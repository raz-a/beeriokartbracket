use beeriokartbracket::Tournament;

fn main() {
    let mut t = Tournament::default();

    if let Some(mut r) = t.registration() {
        r.add_participant("Tony", 0);
    }

    let _res = t.start();

    println!("{:?}", t);
}
