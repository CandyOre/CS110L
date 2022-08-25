use crossbeam_channel as channel;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::new();
    output_vec.resize_with(input_vec.len(), Default::default);

    let (input_sender, input_receiver) = channel::unbounded();
    let (output_sender, output_receiver) = channel::unbounded();

    let mut threads = Vec::new();
    for _ in 0..num_threads - 1 {
        let input_receiver = input_receiver.clone();
        let output_sender = output_sender.clone();
        threads.push(thread::spawn(move || {
            while let Ok((id, val)) = input_receiver.recv() {
                output_sender.send((id, f(val))).expect("");
            }
            drop(output_sender);
        }))
    }
    drop(output_sender);

    while let Some(val) = input_vec.pop() {
        input_sender.send((input_vec.len(), val)).expect("");
    }
    drop(input_sender);

    while let Ok((id, val)) = output_receiver.recv() {
        output_vec[id] = val;
    }

    for thread in threads {
        thread.join().expect("");
    }

    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
