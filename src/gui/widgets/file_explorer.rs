// File Explorer - Visual file manager for SimpleFS

use crate::gui::framebuffer;
use crate::system::fs::filesystem::SimpleFilesystem;
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

const CHAR_WIDTH: u32 = 16;
const CHAR_HEIGHT: u32 = 16;
const LINE_HEIGHT: u32 = 24;
const FILE_ITEM_HEIGHT: u32 = LINE_HEIGHT;
const BUTTON_HEIGHT: u32 = 28;
const BUTTON_SPACING: u32 = 8;
const TOOLBAR_HEIGHT: u32 = BUTTON_HEIGHT + BUTTON_SPACING * 2;

// Colors
const COLOR_TEXT: u32 = 0xFFFFFFFF;           // White text
const COLOR_SELECTED: u32 = 0xFF3366CC;       // Blue selection
const COLOR_HOVER: u32 = 0xFF5588DD;          // Lighter blue hover
const COLOR_BUTTON: u32 = 0xFF3D3D3D;         // Button background
const COLOR_BUTTON_HOVER: u32 = 0xFF5D5D5D;   // Button hover
const COLOR_BUTTON_BORDER: u32 = 0xFF555555;  // Button border

#[derive(Clone)]
struct FileInfo {
    name: String,
    size: usize,
}

pub struct FileExplorer {
    files: Vec<FileInfo>,
    selected_index: Option<usize>,
    scroll_offset: usize,
    visible_height: usize,
    last_click_time: u64,       // For double-click detection
    last_click_index: Option<usize>,
    pub filesystem: Option<SimpleFilesystem>,
    pub device_index: Option<usize>,
}

impl FileExplorer {
    pub fn new() -> Self {
        let mut explorer = FileExplorer {
            files: Vec::new(),
            selected_index: None,
            scroll_offset: 0,
            visible_height: 20, // Default
            last_click_time: 0,
            last_click_index: None,
            filesystem: None,
            device_index: None,
        };

        // Initialize filesystem if block device is available
        unsafe {
            if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                if !devices.is_empty() {
                    let device_idx = 0;
                    let device = &mut devices[device_idx];

                    match SimpleFilesystem::mount(device) {
                        Ok(fs) => {
                            explorer.filesystem = Some(fs);
                            explorer.device_index = Some(device_idx);
                            explorer.refresh_files();

                            // Auto-select first file if any exist
                            if !explorer.files.is_empty() {
                                explorer.selected_index = Some(0);
                            }
                        },
                        Err(_) => {
                            // Filesystem not available
                        }
                    }
                }
            }
        }

        explorer
    }

    /// Refresh file list from filesystem (remounts to get latest changes from disk)
    pub fn refresh_files(&mut self) {
        self.files.clear();

        // Remount filesystem from disk to get latest changes (e.g., from terminal)
        if let Some(device_idx) = self.device_index {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(device_idx) {
                        match SimpleFilesystem::mount(device) {
                            Ok(fs) => {
                                // Update with fresh filesystem from disk
                                self.filesystem = Some(fs);
                            }
                            Err(_) => {
                                // Keep existing filesystem if remount fails
                            }
                        }
                    }
                }
            }
        }

        // Get filesystem instance
        if let Some(ref fs) = self.filesystem {
            // List all files
            let file_list = fs.list_files();

            for entry in file_list {
                self.files.push(FileInfo {
                    name: String::from(entry.get_name()),
                    size: entry.get_size_bytes() as usize,
                });
            }
        }

        // Clear selection if it's out of bounds
        if let Some(idx) = self.selected_index {
            if idx >= self.files.len() {
                self.selected_index = None;
            }
        }
    }

    /// Handle mouse click in content area
    pub fn handle_click(&mut self, x: i32, y: i32, content_height: u32, current_time: u64) -> FileExplorerAction {
        // Check if click is in toolbar area
        if y < TOOLBAR_HEIGHT as i32 {
            return self.handle_toolbar_click(x, y);
        }

        // Calculate which file was clicked (adjust for toolbar)
        let click_y = y - TOOLBAR_HEIGHT as i32;
        if click_y < 0 {
            return FileExplorerAction::None;
        }

        let visible_items = ((content_height - TOOLBAR_HEIGHT) / FILE_ITEM_HEIGHT) as usize;
        self.visible_height = visible_items;

        let clicked_index = (click_y as u32 / FILE_ITEM_HEIGHT) as usize + self.scroll_offset;

        if clicked_index < self.files.len() {
            // Check for double-click (within 500ms)
            let time_diff = current_time.wrapping_sub(self.last_click_time);
            let is_double_click = if let Some(last_idx) = self.last_click_index {
                last_idx == clicked_index && time_diff < 500 && time_diff > 0
            } else {
                false
            };

            self.last_click_index = Some(clicked_index);
            self.last_click_time = current_time;

            if is_double_click {
                // Double-click: open file in editor
                let filename = self.files[clicked_index].name.clone();
                return FileExplorerAction::OpenFile(filename);
            } else {
                // Single click: select file
                self.selected_index = Some(clicked_index);
                return FileExplorerAction::Redraw;
            }
        }

        FileExplorerAction::None
    }

    /// Handle toolbar button clicks
    fn handle_toolbar_click(&self, x: i32, _y: i32) -> FileExplorerAction {
        let mut current_x = BUTTON_SPACING;

        // Refresh button
        let refresh_width = 7 * CHAR_WIDTH + 16; // "Refresh" + padding
        if x >= current_x as i32 && x < (current_x + refresh_width) as i32 {
            return FileExplorerAction::Refresh;
        }
        current_x += refresh_width + BUTTON_SPACING;

        // New File button
        let new_width = 8 * CHAR_WIDTH + 16; // "New File" + padding
        if x >= current_x as i32 && x < (current_x + new_width) as i32 {
            return FileExplorerAction::NewFile;
        }
        current_x += new_width + BUTTON_SPACING;

        // Delete button (only if file selected)
        if self.selected_index.is_some() {
            let delete_width = 6 * CHAR_WIDTH + 16; // "Delete" + padding
            if x >= current_x as i32 && x < (current_x + delete_width) as i32 {
                return FileExplorerAction::DeleteFile;
            }
            current_x += delete_width + BUTTON_SPACING;

            // Rename button (only if file selected)
            let rename_width = 6 * CHAR_WIDTH + 16; // "Rename" + padding
            if x >= current_x as i32 && x < (current_x + rename_width) as i32 {
                return FileExplorerAction::RenameFile;
            }
        }

        FileExplorerAction::None
    }

    /// Scroll the file list
    pub fn scroll(&mut self, lines: i32) {
        if lines > 0 {
            // Scroll down
            let max_scroll = self.files.len().saturating_sub(self.visible_height);
            self.scroll_offset = (self.scroll_offset + lines as usize).min(max_scroll);
        } else if lines < 0 {
            // Scroll up
            self.scroll_offset = self.scroll_offset.saturating_sub((-lines) as usize);
        }
    }

    /// Get selected filename
    pub fn get_selected_filename(&self) -> Option<String> {
        self.selected_index.map(|idx| self.files[idx].name.clone())
    }

    /// Move selection up (arrow up key)
    pub fn move_selection_up(&mut self) {
        if self.files.is_empty() {
            return;
        }

        if let Some(idx) = self.selected_index {
            if idx > 0 {
                self.selected_index = Some(idx - 1);

                // Auto-scroll if needed
                if idx - 1 < self.scroll_offset {
                    self.scroll_offset = idx - 1;
                }
            }
        } else {
            // No selection, select first item
            self.selected_index = Some(0);
            self.scroll_offset = 0;
        }
    }

    /// Move selection down (arrow down key)
    pub fn move_selection_down(&mut self) {
        if self.files.is_empty() {
            return;
        }

        if let Some(idx) = self.selected_index {
            if idx < self.files.len() - 1 {
                self.selected_index = Some(idx + 1);

                // Auto-scroll if needed
                if idx + 1 >= self.scroll_offset + self.visible_height {
                    self.scroll_offset = (idx + 1).saturating_sub(self.visible_height - 1);
                }
            }
        } else {
            // No selection, select first item
            self.selected_index = Some(0);
            self.scroll_offset = 0;
        }
    }

    /// Open selected file (Enter key)
    pub fn open_selected(&self) -> FileExplorerAction {
        if let Some(idx) = self.selected_index {
            if idx < self.files.len() {
                let filename = self.files[idx].name.clone();
                return FileExplorerAction::OpenFile(filename);
            }
        }
        FileExplorerAction::None
    }

    /// Delete selected file
    pub fn delete_selected(&mut self) -> bool {
        if let Some(idx) = self.selected_index {
            if idx < self.files.len() {
                let filename = self.files[idx].name.clone();

                // Delete from filesystem
                if let (Some(ref mut fs), Some(device_idx)) = (&mut self.filesystem, self.device_index) {
                    unsafe {
                        if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                            if let Some(device) = devices.get_mut(device_idx) {
                                if fs.delete_file(device, &filename).is_ok() {
                                    // Remove from our list
                                    self.files.remove(idx);
                                    self.selected_index = None;
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Render the file explorer
    pub fn render_at(&mut self, offset_x: i32, offset_y: i32, width: u32, height: u32, cursor_x: i32, cursor_y: i32) {
        // Calculate visible items
        let visible_items = ((height - TOOLBAR_HEIGHT) / FILE_ITEM_HEIGHT) as usize;
        self.visible_height = visible_items;

        // Draw toolbar
        self.draw_toolbar(offset_x, offset_y, width, cursor_x, cursor_y);

        // Draw file list (below toolbar)
        let list_y = offset_y + TOOLBAR_HEIGHT as i32;
        let visible_end = (self.scroll_offset + visible_items).min(self.files.len());

        for (idx, file_idx) in (self.scroll_offset..visible_end).enumerate() {
            let file = &self.files[file_idx];
            let y = list_y + (idx as i32 * FILE_ITEM_HEIGHT as i32);

            // Check if this item is being hovered
            let is_hovering = cursor_x >= offset_x &&
                              cursor_x < offset_x + width as i32 &&
                              cursor_y >= y &&
                              cursor_y < y + FILE_ITEM_HEIGHT as i32;

            // Determine background color
            let bg_color = if Some(file_idx) == self.selected_index {
                COLOR_SELECTED
            } else if is_hovering {
                COLOR_HOVER
            } else {
                0 // Transparent (use window background)
            };

            // Draw background if selected or hovered
            if bg_color != 0 {
                for dy in 0..FILE_ITEM_HEIGHT {
                    for dx in 0..width {
                        let px = offset_x + dx as i32;
                        let py = y + dy as i32;
                        if px >= 0 && py >= 0 {
                            framebuffer::draw_pixel(px as u32, py as u32, bg_color);
                        }
                    }
                }
            }

            // Draw file icon (simple folder/file emoji)
            let icon = "\u{1F4C4}"; // 📄 document icon
            framebuffer::draw_string((offset_x + 8) as u32, y as u32 + 4, icon, COLOR_TEXT);

            // Draw filename
            let name_x = offset_x + 32; // After icon
            framebuffer::draw_string(name_x as u32, y as u32 + 4, &file.name, COLOR_TEXT);

            // Draw file size (right-aligned)
            let size_str = format_size(file.size);
            let size_width = size_str.len() as u32 * CHAR_WIDTH;
            let size_x = offset_x + width as i32 - size_width as i32 - 8;
            if size_x > name_x {
                framebuffer::draw_string(size_x as u32, y as u32 + 4, &size_str, COLOR_TEXT);
            }
        }

        // Draw scroll indicator if needed
        if self.files.len() > visible_items {
            let scroll_text = alloc::format!("{}-{} of {}",
                self.scroll_offset + 1,
                (self.scroll_offset + visible_items).min(self.files.len()),
                self.files.len()
            );
            let scroll_y = offset_y + height as i32 - CHAR_HEIGHT as i32 - 4;
            framebuffer::draw_string((offset_x + 8) as u32, scroll_y as u32, &scroll_text, COLOR_TEXT);
        }
    }

    /// Draw toolbar with buttons
    fn draw_toolbar(&self, offset_x: i32, offset_y: i32, width: u32, cursor_x: i32, cursor_y: i32) {
        let mut current_x = BUTTON_SPACING;

        // Refresh button
        let refresh_width = 7 * CHAR_WIDTH + 16; // "Refresh" + padding
        self.draw_button(
            offset_x + current_x as i32,
            offset_y + BUTTON_SPACING as i32,
            refresh_width,
            BUTTON_HEIGHT,
            "Refresh",
            cursor_x,
            cursor_y
        );
        current_x += refresh_width + BUTTON_SPACING;

        // New File button
        let new_width = 8 * CHAR_WIDTH + 16; // "New File" + padding
        self.draw_button(
            offset_x + current_x as i32,
            offset_y + BUTTON_SPACING as i32,
            new_width,
            BUTTON_HEIGHT,
            "New File",
            cursor_x,
            cursor_y
        );
        current_x += new_width + BUTTON_SPACING;

        // Delete button (only if file selected)
        if self.selected_index.is_some() {
            let delete_width = 6 * CHAR_WIDTH + 16; // "Delete" + padding
            self.draw_button(
                offset_x + current_x as i32,
                offset_y + BUTTON_SPACING as i32,
                delete_width,
                BUTTON_HEIGHT,
                "Delete",
                cursor_x,
                cursor_y
            );
            current_x += delete_width + BUTTON_SPACING;

            // Rename button (only if file selected)
            let rename_width = 6 * CHAR_WIDTH + 16; // "Rename" + padding
            self.draw_button(
                offset_x + current_x as i32,
                offset_y + BUTTON_SPACING as i32,
                rename_width,
                BUTTON_HEIGHT,
                "Rename",
                cursor_x,
                cursor_y
            );
        }
    }

    /// Draw a button
    fn draw_button(&self, x: i32, y: i32, width: u32, height: u32, label: &str, cursor_x: i32, cursor_y: i32) {
        // Check if button is being hovered
        let is_hovering = cursor_x >= x &&
                          cursor_x < x + width as i32 &&
                          cursor_y >= y &&
                          cursor_y < y + height as i32;

        let bg_color = if is_hovering {
            COLOR_BUTTON_HOVER
        } else {
            COLOR_BUTTON
        };

        // Draw button border
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && py >= 0 {
                    let color = if dx == 0 || dy == 0 || dx == width - 1 || dy == height - 1 {
                        COLOR_BUTTON_BORDER
                    } else {
                        bg_color
                    };
                    framebuffer::draw_pixel(px as u32, py as u32, color);
                }
            }
        }

        // Draw button label (centered)
        let text_width = label.len() as u32 * CHAR_WIDTH;
        let text_x = x + ((width - text_width) / 2) as i32;
        let text_y = y + ((height - CHAR_HEIGHT) / 2) as i32;
        framebuffer::draw_string(text_x as u32, text_y as u32, label, COLOR_TEXT);
    }
}

/// Actions that can be returned from the file explorer
pub enum FileExplorerAction {
    None,
    Redraw,
    Refresh,
    NewFile,
    DeleteFile,
    RenameFile,
    OpenFile(String),
}

/// Format file size in human-readable format
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        alloc::format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        alloc::format!("{} KB", bytes / 1024)
    } else {
        alloc::format!("{} MB", bytes / (1024 * 1024))
    }
}

/// Global file explorer instances
static mut FILE_EXPLORERS: Vec<FileExplorer> = Vec::new();

pub fn init() {
    // Nothing to do - explorers are created on demand
}

/// Create a new file explorer instance and return its ID
pub fn create_file_explorer() -> usize {
    unsafe {
        FILE_EXPLORERS.push(FileExplorer::new());
        FILE_EXPLORERS.len() - 1
    }
}

/// Remove a file explorer instance by ID
pub fn remove_file_explorer(id: usize) {
    unsafe {
        if id < FILE_EXPLORERS.len() {
            FILE_EXPLORERS.remove(id);
        }
    }
}

/// Get a file explorer instance by ID
pub fn get_file_explorer(id: usize) -> Option<&'static mut FileExplorer> {
    unsafe {
        FILE_EXPLORERS.get_mut(id)
    }
}

/// Get all file explorer instance IDs
pub fn get_all_file_explorers() -> Vec<usize> {
    unsafe {
        (0..FILE_EXPLORERS.len()).collect()
    }
}

/// Render a file explorer instance
pub fn render_at(id: usize, offset_x: i32, offset_y: i32, width: u32, height: u32) {
    let (cursor_x, cursor_y) = framebuffer::get_cursor_pos();
    if let Some(explorer) = get_file_explorer(id) {
        explorer.render_at(offset_x, offset_y, width, height, cursor_x, cursor_y);
    }
}

/// Handle click in file explorer
pub fn handle_click(id: usize, x: i32, y: i32, content_height: u32, current_time: u64) -> FileExplorerAction {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.handle_click(x, y, content_height, current_time)
    } else {
        FileExplorerAction::None
    }
}

/// Refresh file list
pub fn refresh(id: usize) {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.refresh_files();
    }
}

/// Delete selected file
pub fn delete_selected(id: usize) -> bool {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.delete_selected()
    } else {
        false
    }
}

/// Scroll file explorer
pub fn scroll(id: usize, lines: i32) {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.scroll(lines);
    }
}

/// Move selection up (Arrow Up key)
pub fn move_selection_up(id: usize) {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.move_selection_up();
    }
}

/// Move selection down (Arrow Down key)
pub fn move_selection_down(id: usize) {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.move_selection_down();
    }
}

/// Open selected file (Enter key)
pub fn open_selected(id: usize) -> FileExplorerAction {
    if let Some(explorer) = get_file_explorer(id) {
        explorer.open_selected()
    } else {
        FileExplorerAction::None
    }
}

/// Select a file by name (used after rename to keep selection)
pub fn select_file_by_name(id: usize, filename: &str) {
    if let Some(explorer) = get_file_explorer(id) {
        // Find the file in the list
        for (idx, file) in explorer.files.iter().enumerate() {
            if file.name == filename {
                explorer.selected_index = Some(idx);

                // Scroll to make sure the file is visible
                if idx < explorer.scroll_offset {
                    explorer.scroll_offset = idx;
                } else if idx >= explorer.scroll_offset + explorer.visible_height {
                    explorer.scroll_offset = idx.saturating_sub(explorer.visible_height - 1);
                }
                break;
            }
        }
    }
}

/// Refresh all open file explorers (called when filesystem changes from terminal)
pub fn refresh_all_explorers() {
    unsafe {
        for explorer in FILE_EXPLORERS.iter_mut() {
            explorer.refresh_files();
        }
    }
}
