mod request;
mod response;

use clap::Parser;
use rand::{Rng, SeedableRng};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, Mutex};
use std::sync::Arc;
use std::time::Duration;
use std::collections::{HashSet, HashMap};
use futures::stream::{self, StreamExt};

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(
        short,
        long,
        about = "IP/port to bind to",
        default_value = "0.0.0.0:1100"
    )]
    bind: String,
    #[clap(short, long, multiple_occurrences = true, about = "Upstream host to forward requests to")]
    upstream: Vec<String>,
    #[clap(
        long,
        about = "Perform active health checks on this interval (in seconds)",
        default_value = "10"
    )]
    active_health_check_interval: usize,
    #[clap(
    long,
    about = "Path to send request to for active health checks",
    default_value = "/"
    )]
    active_health_check_path: String,
    #[clap(
        long,
        about = "Maximum number of requests to accept per IP per minute (0 = unlimited)",
        default_value = "0"
    )]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: RwLock<Vec<String>>,
    /// Addresses of servers that are currently dead
    dead_addresses: RwLock<Vec<String>>,
    /// Requests per minutes
    requests_counter: Mutex<HashMap<String, usize>>,
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let mut listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = Arc::new(ProxyState {
        upstream_addresses: RwLock::new(options.upstream),
        dead_addresses: RwLock::new(Vec::new()),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        requests_counter: Mutex::new(HashMap::new()),
        max_requests_per_minute: options.max_requests_per_minute,
    });

    let state_ref = state.clone();
    tokio::spawn(async move {
        active_health_check(&state_ref).await;
    });

    if options.max_requests_per_minute > 0 {
        let state_ref = state.clone();
        tokio::spawn(async move {
            requests_counter_timer(&state_ref, Duration::from_secs(60)).await;
        });
    }

    while let Some(Ok(stream)) = listener.next().await {
        let state_ref = state.clone();
        tokio::spawn(async move {
            handle_connection(stream, &state_ref).await;
        });
    }
}

async fn requests_counter_timer(state: &ProxyState, duration: Duration) {
    loop {
        tokio::time::delay_for(duration).await;
        state.requests_counter.lock().await.clear();
        log::debug!("Request counter reset!");
    }
}

async fn get_random_upstream(state: &ProxyState) -> Option<(usize, String)> {
    let upstream = state.upstream_addresses.read().await;
    if upstream.len() > 0 {
        let mut rng = rand::rngs::StdRng::from_entropy();
        let upstream_idx = rng.gen_range(0, upstream.len());
        let upstream_ip = &upstream[upstream_idx];
        Some((upstream_idx, upstream_ip.to_string()))
    } else {
        None
    }
}

async fn delete_upstream(state: &ProxyState, idx: usize) {
    let mut upstream = state.upstream_addresses.write().await;
    let mut dead = state.dead_addresses.write().await;
    if idx < upstream.len() {
        dead.push(std::mem::take(&mut upstream[idx]));
        upstream.swap_remove(idx);
    }
}

async fn is_alive(state: &ProxyState, ip: &String) -> Option<()> {
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(&state.active_health_check_path)
        .header("Host", ip)
        .body(Vec::new())
        .unwrap();
    let mut stream = TcpStream::connect(ip).await.ok()?;

    request::write_to_stream(&request, &mut stream).await.ok()?;
    if response::read_from_stream(&mut stream, &http::Method::GET)
        .await
        .ok()?
        .status() == http::StatusCode::OK {
            Some(())
    } else {
        None
    }
}

async fn filter_alive(state: &ProxyState) {
    let mut upstream = state.upstream_addresses.write().await;
    let mut dead = state.dead_addresses.write().await;
    upstream.append(&mut *dead);

    log::debug!("Health Check Start with {} alive!", upstream.len());
    *dead = upstream.clone().into_iter().collect();
    *upstream = stream::iter(std::mem::take(&mut *upstream))
        .filter_map(|ip| async {
            if let Some(_) = is_alive(state, &ip).await {
                Some(ip)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .await;

    let alive: HashSet<String> = upstream.clone().into_iter().collect();
    *dead = std::mem::take(&mut *dead)
        .into_iter()
        .filter(|ip| !alive.contains(ip))
        .collect();
    log::debug!("Health Check Complete with {} alive!", alive.len());
}

async fn active_health_check(state: &ProxyState) {
    let duration = Duration::from_secs(state.active_health_check_interval as u64);
    loop {
        tokio::time::delay_for(duration).await;
        filter_alive(state).await;
    }
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    loop {
        if let Some((idx, ip)) = get_random_upstream(state).await {
            match TcpStream::connect(ip).await {
                Ok(stream) => return Ok(stream),
                Err(_) => {
                    delete_upstream(state, idx).await;
                }
            }
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No Alive Upstream Server!",
            ))
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("{} <- {}", client_ip, response::format_response_line(&response));
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = client_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        if state.max_requests_per_minute > 0 {
            log::debug!("Check requests limit!");
            let mut counter = state.requests_counter.lock().await;
            let count = counter.entry(client_ip.clone()).or_insert(0);
            *count += 1;
            if *count > state.max_requests_per_minute {
                let response = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
                send_response(&mut client_conn, &response).await;
                // response::write_to_stream(&response, &mut client_conn).await.unwrap();
                return;
            }
        }

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!("Failed to send request to upstream {}: {}", upstream_ip, error);
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
