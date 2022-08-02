/*
// problem 1
fn main() {
    let mut s = String::from("hello");
    let ref1 = &s;
    let ref2 = &ref1;
    let ref3 = &ref2;
    // s = String::from("goodbye");
    println!("{}", ref3.to_uppercase());
}
*/

/*
// problem 2
fn drip_drop<'a>() -> &'a String {
    let s = String::from("hello world!");
    return &s;
}
*/

// problem 3
fn main() {
    let s1 = String::from("hello");
    let mut v = Vec::new();
    v.push(s1);
    let s2: &String = &v[0];
    println!("{}", s2);
}