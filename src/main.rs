mod tornado;
use tornado::rotate;

fn main() {
    for i in std::env::args().skip(1){
        rotate(tornado::Direction::Left, &std::path::PathBuf::from(i));
    }
}
