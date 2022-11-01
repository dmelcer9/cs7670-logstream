use std::env;
use std::fs::canonicalize;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use notify::{Event, EventHandler, EventKind, recommended_watcher, RecursiveMode, Watcher};
use notify::event::{CreateKind, ModifyKind, RemoveKind};

struct FilesysStreamProgram {
    src: PathBuf,
    dest: PathBuf
}

impl FilesysStreamProgram {
    fn handle_create(&mut self, path: &Path, kind: CreateKind) {
        println!("Create {:?} {:?}", path, kind);
    }
    fn handle_modify(&mut self, paths: Vec<PathBuf>, kind: ModifyKind) {
        println!("Modify {:?} {:?}", paths, kind);
    }
    fn handle_remove(&mut self, path: &Path, kind: RemoveKind) {
        println!("Remove {:?} {:?}", path, kind);
    }
}

impl EventHandler for FilesysStreamProgram{
    fn handle_event(&mut self, event: notify::Result<Event>) {
        if let Ok(event) = event {
            paths : Vec<PathBuf> = event.paths.iter().map(|p| p.strip_prefix(&self.src).unwrap()).collect();
            match event.kind {
                EventKind::Create(kind) => {
                    self.handle_create(paths[0], kind)
                }
                EventKind::Modify(kind) => {
                    self.handle_modify(paths, kind)
                }
                EventKind::Remove(kind) => {
                    self.handle_remove(paths[0], kind)
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let source = canonicalize(&args[1]).unwrap();
    println!("Canonical Source: {:?}", source);
    let dest = canonicalize(&args[2]).unwrap();
    println!("Canonical Dest: {:?}", dest);

    let event_handler = FilesysStreamProgram{};

    let mut watcher = recommended_watcher(event_handler).unwrap();

    watcher.watch(&source, RecursiveMode::Recursive).unwrap();

    loop {
        sleep(Duration::from_secs(1));
    }
}
