use beeriokartbracket::Tournament;

fn main() {
    let mut t = Tournament::default();

    if let Some(mut r) = t.get_registration() {
        r.add_participant("Tony", 0);
        r.start();
    }

    println!("{:?}", t);
}
