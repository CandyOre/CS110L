use linked_list::LinkedList;

use crate::linked_list::ComputeNorm;
pub mod linked_list;

fn main() {
    let mut list: LinkedList<String> = LinkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.get_size(), 0);
    for i in 1..12 {
        list.push_front(i.to_string() + "w");
    }
    println!("{}", list);
    println!("list size: {}", list.get_size());
    println!("top element: {}", list.pop_front().unwrap());
    println!("{}", list);
    println!("size: {}", list.get_size());
    println!("{}", list.to_string()); // ToString impl for anything impl Display

    for val in &list {
       println!("{}", val);
    }

    let mut list: LinkedList<f64> = LinkedList::new();
    for i in 1..3 {
        list.push_front(i as f64);
    }
    println!("{}, Norm: {}", list, list.compute_norm());

    let list2 = list.clone();
    println!("{}", list2);

    println!("{}", (list == list2).to_string());
}
