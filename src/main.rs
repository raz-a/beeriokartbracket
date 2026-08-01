use beeriokartbracket::Tournament;

fn main() {
    let mut t = Tournament::default();

    let _ = t.add_participant("Tony");
    println!("{:?}", t);
}
