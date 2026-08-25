use fixer_core::{Isbn13, MovieRelease, ReleaseDate};

fn main() {
    let isbn = Isbn13::new("9780547773742").unwrap();
    let date = ReleaseDate::ymd(2000, 9, 29).unwrap();
    let _release = MovieRelease::new(isbn, date);
}
