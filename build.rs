use std::env;
use std::fs;
use std::path::Path;

fn update_local_git_hook() {
    let p = env::current_dir().unwrap();
    let origin_path = Path::new(&p).join("./tools/pre-commit");
    let dest_path = Path::new(&p).join(".git/hooks/pre-commit");

    fs::copy(&origin_path, &dest_path).unwrap();
}

fn main() {
    update_local_git_hook();
}
