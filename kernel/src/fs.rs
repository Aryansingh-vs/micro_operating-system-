use alloc::{format, string::String, vec::Vec};
use spin::Mutex;

#[derive(Clone)]
pub struct File {
    pub name: String,
    pub data: String,
}

#[derive(Clone)]
pub struct Directory {
    pub name: String,
    pub dirs: Vec<Directory>,
    pub files: Vec<File>,
}

impl Directory {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            dirs: Vec::new(),
            files: Vec::new(),
        }
    }

}

pub static ROOT: Mutex<Directory> = Mutex::new(Directory {
    name: String::new(),
    dirs: Vec::new(),
    files: Vec::new(),
});

pub fn init() {
    let mut root = Directory::new("/");
    let mut docs = Directory::new("docs");
    docs.files.push(File {
        name: String::from("readme.txt"),
        data: String::from("Welcome to AryOS! This file lives in the in-memory filesystem."),
    });
    docs.files.push(File {
        name: String::from("college.txt"),
        data: String::from("AryOS: a small x86_64 operating system written in Rust."),
    });
    root.dirs.push(docs);
    root.dirs.push(Directory::new("bin"));
    root.dirs.push(Directory::new("tmp"));
    root.files.push(File {
        name: String::from("welcome.txt"),
        data: String::from("AryOS is ready."),
    });
    *ROOT.lock() = root;
}

pub fn list_cwd(cwd: &[String]) -> Vec<String> {
    let root = ROOT.lock();
    let mut dir = &*root;
    for component in cwd {
        if let Some(next) = dir.dirs.iter().find(|d| d.name == *component) {
            dir = next;
        } else {
            return Vec::new();
        }
    }
    let mut out = Vec::new();
    for d in &dir.dirs {
        out.push(format!("{}/", d.name));
    }
    for f in &dir.files {
        out.push(f.name.clone());
    }
    out
}

pub fn find_dir_mut(cwd: &[String]) -> Option<*mut Directory> {
    let mut root = ROOT.lock();
    let mut ptr: *mut Directory = &mut *root;
    for component in cwd {
        let next = unsafe {
            let dir = &mut *ptr;
            dir.dirs.iter_mut().find(|d| d.name == *component)
                .map(|d| d as *mut Directory)?
        };
        ptr = next;
    }
    Some(ptr)
}

pub fn create_file(cwd: &[String], name: &str, data: &str) -> bool {
    if name.is_empty() { return false; }
    unsafe {
        let ptr = match find_dir_mut(cwd) { Some(ptr) => ptr, None => return false };
        let dir = &mut *ptr;
        if dir.files.iter().any(|f| f.name == name) { return false; }
        dir.files.push(File { name: String::from(name), data: String::from(data) });
        true
    }
}

pub fn write_file(cwd: &[String], name: &str, data: &str) -> bool {
    unsafe {
        let ptr = match find_dir_mut(cwd) { Some(ptr) => ptr, None => return false };
        let dir = &mut *ptr;
        if let Some(file) = dir.files.iter_mut().find(|f| f.name == name) {
            file.data = String::from(data);
            true
        } else {
            false
        }
    }
}

pub fn remove_file(cwd: &[String], name: &str) -> bool {
    unsafe {
        let ptr = match find_dir_mut(cwd) { Some(ptr) => ptr, None => return false };
        let dir = &mut *ptr;
        if let Some(index) = dir.files.iter().position(|f| f.name == name) {
            dir.files.remove(index);
            true
        } else {
            false
        }
    }
}

pub fn create_dir(cwd: &[String], name: &str) -> bool {
    if name.is_empty() { return false; }
    unsafe {
        let ptr = match find_dir_mut(cwd) { Some(ptr) => ptr, None => return false };
        let dir = &mut *ptr;
        if dir.dirs.iter().any(|d| d.name == name) { return false; }
        dir.dirs.push(Directory::new(name));
        true
    }
}
