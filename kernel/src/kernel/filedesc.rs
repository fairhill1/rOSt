// File descriptor management for user processes

use alloc::string::String;
use alloc::vec::Vec;

/// Maximum number of file descriptors per process
pub const MAX_FDS: usize = 64;

/// Standard file descriptors
pub const STDIN_FD: i32 = 0;
pub const STDOUT_FD: i32 = 1;
pub const STDERR_FD: i32 = 2;

/// File descriptor entry
#[derive(Clone)]
pub struct FileDescriptor {
    /// File name in SimpleFS
    pub file_name: String,
    /// Current read/write offset
    pub offset: usize,
    /// Open flags (from syscall)
    pub flags: u32,
    /// Is this FD currently open?
    pub is_open: bool,
}

impl FileDescriptor {
    pub fn new(file_name: String, flags: u32) -> Self {
        Self {
            file_name,
            offset: 0,
            flags,
            is_open: true,
        }
    }
}

/// Per-process file descriptor table
pub struct FileDescriptorTable {
    fds: Vec<Option<FileDescriptor>>,
}

impl FileDescriptorTable {
    /// Create new FD table with stdin/stdout/stderr pre-opened
    pub fn new() -> Self {
        let mut fds = Vec::new();
        fds.resize_with(MAX_FDS, || None);

        // Reserve 0, 1, 2 for stdin/stdout/stderr (special handling in syscalls)
        fds[STDIN_FD as usize] = Some(FileDescriptor::new(String::from("<stdin>"), 0));
        fds[STDOUT_FD as usize] = Some(FileDescriptor::new(String::from("<stdout>"), 1));
        fds[STDERR_FD as usize] = Some(FileDescriptor::new(String::from("<stderr>"), 1));

        Self { fds }
    }

    /// Allocate a new file descriptor, returns FD number or None if table full
    pub fn alloc(&mut self, file_name: String, flags: u32) -> Option<i32> {
        // Start from FD 3 (after stdin/stdout/stderr)
        for fd in 3..MAX_FDS {
            if self.fds[fd].is_none() {
                self.fds[fd] = Some(FileDescriptor::new(file_name, flags));
                return Some(fd as i32);
            }
        }
        None // Table full
    }

    /// Get mutable reference to FD
    pub fn get_mut(&mut self, fd: i32) -> Option<&mut FileDescriptor> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return None;
        }
        self.fds[fd as usize].as_mut()
    }

    /// Get immutable reference to FD
    pub fn get(&self, fd: i32) -> Option<&FileDescriptor> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return None;
        }
        self.fds[fd as usize].as_ref()
    }

    /// Close a file descriptor
    pub fn close(&mut self, fd: i32) -> bool {
        if fd < 3 || fd as usize >= MAX_FDS {
            return false; // Can't close stdin/stdout/stderr
        }
        if self.fds[fd as usize].is_some() {
            self.fds[fd as usize] = None;
            true
        } else {
            false
        }
    }

    /// Check if FD is valid and open
    pub fn is_valid(&self, fd: i32) -> bool {
        self.get(fd).is_some()
    }
}

impl Default for FileDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}
