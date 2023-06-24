#![feature(test)]

extern crate test;


#[test]
fn cat() {}

#[test]
fn dog() {
    panic!("was not a good boy");
}

#[test]
#[ignore]
fn frog() {}

#[test]
#[ignore]
fn owl() {
    panic!("broke neck");
}
