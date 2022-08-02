/* The following exercises were borrowed from Will Crichton's CS 242 Rust lab. */

#[allow(unused_imports)]
use std::collections::HashSet;

fn main() {
    println!("Hi! Try running \"cargo test\" to run tests.");
}

#[cfg(test)]
fn add_n(v: Vec<i32>, n: i32) -> Vec<i32> {
    let mut new_v: Vec<i32> = Vec::new();
    for e in v.iter() {
        new_v.push(*e + n);
    }
    new_v
}

#[cfg(test)]
fn add_n_inplace(v: &mut Vec<i32>, n: i32) {
    for e in v.iter_mut() {
        *e += n;
    }
}

#[cfg(test)]
fn dedup(v: &mut Vec<i32>) {
    let mut set: HashSet<i32> = HashSet::new();
    let mut v_new: Vec<i32> = Vec::new();
    for e in v.iter() {
        if !set.contains(e) {
            set.insert(*e);
            v_new.push(*e);
        }
    }
    *v = v_new;
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_add_n() {
        assert_eq!(add_n(vec![1], 2), vec![3]);
    }

    #[test]
    fn test_add_n_inplace() {
        let mut v = vec![1];
        add_n_inplace(&mut v, 2);
        assert_eq!(v, vec![3]);
    }

    #[test]
    fn test_dedup() {
        let mut v = vec![3, 1, 0, 1, 4, 4];
        dedup(&mut v);
        assert_eq!(v, vec![3, 1, 0, 4]);
    }
}
