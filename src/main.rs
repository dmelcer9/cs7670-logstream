use std::collections::HashMap;
use std::env;
use std::fs::{canonicalize, create_dir, DirEntry, File, OpenOptions, remove_dir, remove_file};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use std::io;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use notify::{Event, EventHandler, EventKind, recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use notify::event::{CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind, RenameMode};

struct AppendFile {
    inner_file: File,
    len: usize
}


impl AppendFile {
    fn new(absolute_path: &Path, len: usize) -> Self {
        let inner_file = OpenOptions::new().create(true).append(true).open(absolute_path).unwrap();
        Self {
            inner_file, len
        }
    }
}

impl Write for AppendFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.len += buf.len();
        self.inner_file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner_file.flush()
    }
}

struct FilesysStreamProgram {
    src: PathBuf,
    dest: PathBuf,
    entries: HashMap<PathBuf, AppendFile>
}


fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&DirEntry)) -> io::Result<()> {

    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            cb(&entry);
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            }

        }
    }
    Ok(())
}

impl FilesysStreamProgram {
    fn new(src: PathBuf, dest: PathBuf) -> Self {
        let src = canonicalize(src).unwrap();
        println!("Canonical Source: {:?}", src);
        let dest = canonicalize(dest).unwrap();
        println!("Canonical Dest: {:?}", dest);

        let mut entries = HashMap::new();

        visit_dirs(&src, &mut |dirent| {
            let rel_path = dirent.path().strip_prefix(&src).unwrap().to_path_buf();
            let dest_path_abs = dest.join(&rel_path);
            if dirent.path().is_dir(){
                if !dest_path_abs.exists() {
                    create_dir(dest_path_abs).unwrap();
                }
            } else {
                let size = dirent.metadata().unwrap().len() as usize;
                entries.insert(rel_path.clone(), AppendFile::new(&dest_path_abs, size));
            }
        }).unwrap();

        Self{src, dest, entries}
    }

    fn update_file_contents(&mut self, rel_path: &Path) {
        let src_path = self.src.join(&rel_path);
        if src_path.exists() {
            let src_len = src_path.metadata().unwrap().len() as usize;
            let mut entry = match self.entries.get_mut(rel_path) {
                Some(e) => e,
                None => {
                    self.create_file(rel_path);
                    self.entries.get_mut(&rel_path.to_path_buf()).unwrap()
                }
            };

            if src_len <= entry.len {
                entry.len = src_len;
            } else {
                let num_tail_bytes = src_len - entry.len;
                let mut src_file = File::open(src_path).unwrap();
                src_file.seek(SeekFrom::Start(entry.len as u64)).unwrap();
                let mut buffer = vec![0u8; num_tail_bytes];
                src_file.read(&mut buffer).unwrap();
                entry.write(&buffer).unwrap();
            }
        }

    }

    fn create_file(&mut self, rel_path: &Path) {
        self.entries.insert(rel_path.to_path_buf(), AppendFile::new(&self.dest.join(rel_path), 0));
    }

    fn remove_file(&mut self, rel_path: &Path){
        self.entries.remove(rel_path).unwrap();
        let dest_path = self.dest.join(rel_path);
        if dest_path.exists() {
            fs::remove_file(self.dest.join(rel_path)).unwrap();
        }
    }

    fn handle_create(&mut self, path: &Path, kind: CreateKind) {
        match kind {
            CreateKind::File => {
               self.create_file(path);
            }
            CreateKind::Folder => {
                let dest_path = self.dest.join(path);
                if !dest_path.exists(){
                    create_dir(self.dest.join(path)).unwrap()}
                }
            _ => {}
        }
        println!("Create {:?} {:?}", path, kind);
    }
    fn handle_modify(&mut self, paths: Vec<PathBuf>, kind: ModifyKind) {
        match kind {
            ModifyKind::Data(_) => {
                self.update_file_contents(&paths[0]);
            }
            ModifyKind::Name(nckind) => {
                let real_nckind = match nckind {
                    RenameMode::Any => {
                        if paths.len() > 1 {
                            RenameMode::Both
                        } else if self.src.join(&paths[0]).exists() {
                            RenameMode::To
                        } else {
                            RenameMode::From
                        }
                    },
                    other => other
                };

                match real_nckind{
                    RenameMode::To => {
                        self.create_file(&paths[0]);
                    }
                    RenameMode::From => {
                        self.remove_file(&paths[0]);
                    }
                    RenameMode::Both => {
                        let file = self.entries.remove(&paths[0]).unwrap();
                        self.entries.insert(paths[1].to_path_buf(), file);

                        fs::rename(self.dest.join(&paths[0]), self.dest.join(&paths[1])).unwrap();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        println!("Modify {:?} {:?}", paths, kind);
    }
    fn handle_remove(&mut self, path: &Path, kind: RemoveKind) {
        let dest_path = self.dest.join(path);
        match kind {
            RemoveKind::File => {self.remove_file(path)}
            RemoveKind::Folder => {
                if dest_path.exists(){
                    remove_dir(dest_path).unwrap();
                }
            }
            _ => {}
        }
        println!("Remove {:?} {:?}", path, kind);
    }
}

impl EventHandler for FilesysStreamProgram{
    fn handle_event(&mut self, event: notify::Result<Event>) {
        if let Ok(event) = event {
            let paths : Vec<PathBuf> = event.paths.iter().map(|p| p.strip_prefix(&self.src).unwrap().to_path_buf()).collect();
            match event.kind {
                EventKind::Create(kind) => {
                    self.handle_create(&paths[0], kind)
                }
                EventKind::Modify(kind) => {
                    self.handle_modify(paths, kind)
                }
                EventKind::Remove(kind) => {
                    self.handle_remove(&paths[0], kind)
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let event_handler = FilesysStreamProgram::new((&args[1]).parse().unwrap(), (&args[2]).parse().unwrap());

    let mut watcher = recommended_watcher(event_handler).unwrap();
    println!("Using {:?} watcher", RecommendedWatcher::kind());

    watcher.watch((&args[1]).as_ref(), RecursiveMode::Recursive).unwrap();

    loop {
        sleep(Duration::from_secs(1));
    }
}
