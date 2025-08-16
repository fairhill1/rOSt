use alloc::{vec::Vec, string::{String, ToString}, collections::BTreeMap, format};

#[derive(Debug, Clone)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: usize,
    pub data: Vec<u8>,
    pub children: BTreeMap<String, FileEntry>,
}

impl FileEntry {
    pub fn new_file(name: String, data: Vec<u8>) -> Self {
        let size = data.len();
        FileEntry {
            name,
            file_type: FileType::File,
            size,
            data,
            children: BTreeMap::new(),
        }
    }
    
    pub fn new_directory(name: String) -> Self {
        FileEntry {
            name,
            file_type: FileType::Directory,
            size: 0,
            data: Vec::new(),
            children: BTreeMap::new(),
        }
    }
    
    pub fn is_file(&self) -> bool {
        matches!(self.file_type, FileType::File)
    }
    
    pub fn is_directory(&self) -> bool {
        matches!(self.file_type, FileType::Directory)
    }
}

pub struct FileSystem {
    root: FileEntry,
    current_path: Vec<String>,
}

impl FileSystem {
    pub fn new() -> Self {
        let mut fs = FileSystem {
            root: FileEntry::new_directory("root".to_string()),
            current_path: Vec::new(),
        };
        
        // Create some initial directories and files
        fs.mkdir("bin").ok();
        fs.mkdir("etc").ok();
        fs.mkdir("tmp").ok();
        fs.mkdir("home").ok();
        
        // Create some demo files
        fs.create_file("hello.txt", "Hello from Rust OS filesystem!\nThis is a test file.\n".as_bytes()).ok();
        fs.create_file("readme.md", "# Rust OS\n\nA simple operating system written in Rust.\n".as_bytes()).ok();
        
        // Create files in etc
        fs.cd("etc").ok();
        fs.create_file("motd", "Welcome to Rust OS!\n".as_bytes()).ok();
        fs.create_file("version", "Rust OS v0.1.0\n".as_bytes()).ok();
        fs.cd("..").ok();
        
        fs
    }
    
    fn get_current_dir(&mut self) -> Result<&mut FileEntry, &'static str> {
        let mut current = &mut self.root;
        for path_component in &self.current_path {
            if let Some(entry) = current.children.get_mut(path_component) {
                if entry.is_directory() {
                    current = entry;
                } else {
                    return Err("Path component is not a directory");
                }
            } else {
                return Err("Path not found");
            }
        }
        Ok(current)
    }
    
    fn get_current_dir_readonly(&self) -> Result<&FileEntry, &'static str> {
        let mut current = &self.root;
        for path_component in &self.current_path {
            if let Some(entry) = current.children.get(path_component) {
                if entry.is_directory() {
                    current = entry;
                } else {
                    return Err("Path component is not a directory");
                }
            } else {
                return Err("Path not found");
            }
        }
        Ok(current)
    }
    
    pub fn ls(&self) -> Result<Vec<(String, FileType, usize)>, &'static str> {
        let current_dir = self.get_current_dir_readonly()?;
        let mut entries = Vec::new();
        
        for (name, entry) in &current_dir.children {
            entries.push((name.clone(), entry.file_type.clone(), entry.size));
        }
        
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }
    
    pub fn mkdir(&mut self, name: &str) -> Result<(), &'static str> {
        if name.contains('/') || name == "." || name == ".." {
            return Err("Invalid directory name");
        }
        
        let current_dir = self.get_current_dir()?;
        
        if current_dir.children.contains_key(name) {
            return Err("Directory already exists");
        }
        
        current_dir.children.insert(
            name.to_string(),
            FileEntry::new_directory(name.to_string())
        );
        
        Ok(())
    }
    
    pub fn create_file(&mut self, name: &str, data: &[u8]) -> Result<(), &'static str> {
        if name.contains('/') || name == "." || name == ".." {
            return Err("Invalid file name");
        }
        
        let current_dir = self.get_current_dir()?;
        
        current_dir.children.insert(
            name.to_string(),
            FileEntry::new_file(name.to_string(), data.to_vec())
        );
        
        Ok(())
    }
    
    pub fn read_file(&self, name: &str) -> Result<&[u8], &'static str> {
        let current_dir = self.get_current_dir_readonly()?;
        
        if let Some(entry) = current_dir.children.get(name) {
            if entry.is_file() {
                Ok(&entry.data)
            } else {
                Err("Not a file")
            }
        } else {
            Err("File not found")
        }
    }
    
    pub fn remove(&mut self, name: &str) -> Result<(), &'static str> {
        let current_dir = self.get_current_dir()?;
        
        if current_dir.children.remove(name).is_some() {
            Ok(())
        } else {
            Err("File or directory not found")
        }
    }
    
    pub fn cd(&mut self, path: &str) -> Result<(), &'static str> {
        if path == ".." {
            if !self.current_path.is_empty() {
                self.current_path.pop();
            }
            return Ok(());
        }
        
        if path == "." {
            return Ok(());
        }
        
        if path == "/" {
            self.current_path.clear();
            return Ok(());
        }
        
        // Check if the directory exists
        let current_dir = self.get_current_dir_readonly()?;
        if let Some(entry) = current_dir.children.get(path) {
            if entry.is_directory() {
                self.current_path.push(path.to_string());
                Ok(())
            } else {
                Err("Not a directory")
            }
        } else {
            Err("Directory not found")
        }
    }
    
    pub fn pwd(&self) -> String {
        if self.current_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.current_path.join("/"))
        }
    }
    
    pub fn stat(&self, name: &str) -> Result<(FileType, usize), &'static str> {
        let current_dir = self.get_current_dir_readonly()?;
        
        if let Some(entry) = current_dir.children.get(name) {
            Ok((entry.file_type.clone(), entry.size))
        } else {
            Err("File or directory not found")
        }
    }
}

static mut FILESYSTEM: Option<FileSystem> = None;

pub fn init() {
    unsafe {
        FILESYSTEM = Some(FileSystem::new());
    }
    crate::uart::Uart::new(0x0900_0000).puts("Filesystem initialized\n");
}

pub fn get_fs() -> &'static mut FileSystem {
    unsafe {
        FILESYSTEM.as_mut().expect("Filesystem not initialized")
    }
}

// Convenience functions for shell commands
pub fn ls() -> Result<Vec<(String, FileType, usize)>, &'static str> {
    get_fs().ls()
}

pub fn mkdir(name: &str) -> Result<(), &'static str> {
    get_fs().mkdir(name)
}

pub fn create_file(name: &str, data: &[u8]) -> Result<(), &'static str> {
    get_fs().create_file(name, data)
}

pub fn read_file(name: &str) -> Result<&[u8], &'static str> {
    get_fs().read_file(name)
}

pub fn remove(name: &str) -> Result<(), &'static str> {
    get_fs().remove(name)
}

pub fn cd(path: &str) -> Result<(), &'static str> {
    get_fs().cd(path)
}

pub fn pwd() -> String {
    get_fs().pwd()
}

pub fn stat(name: &str) -> Result<(FileType, usize), &'static str> {
    get_fs().stat(name)
}