# GUI Implementation Plan

## Goal
Build a simple graphical user interface with window manager and file browser.

## Phase 1: Drawing Primitives (Start Here)

### 1.1 Basic Shapes
Create `src/kernel/graphics.rs` with:
- `fill_rect(x, y, width, height, color)` - Fill rectangle
- `draw_rect(x, y, width, height, color)` - Outline rectangle
- `draw_line(x1, y1, x2, y2, color)` - Line drawing
- `draw_char(x, y, ch, color)` - Character rendering
- `draw_text(x, y, text, color)` - String rendering

### 1.2 Color System
```rust
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255 };
    pub const GRAY: Color = Color { r: 200, g: 200, b: 200 };
}
```

### 1.3 Font Rendering
Use embedded bitmap font (8x16 or similar):
- Include font data as const array
- Draw character by rendering pixels from bitmap
- Support basic ASCII (32-126)

## Phase 2: Widget System

### 2.1 Widget Trait
```rust
pub trait Widget {
    fn draw(&self, fb: &mut Framebuffer);
    fn handle_click(&mut self, x: i32, y: i32) -> bool;
    fn handle_key(&mut self, key: u8) -> bool;
    fn bounds(&self) -> Rect;
}
```

### 2.2 Basic Widgets
- **Label** - Static text display
- **Button** - Clickable button with callback
- **TextBox** - Single-line text input
- **ListBox** - Scrollable list of items

### 2.3 Layout
- **Rect** - x, y, width, height
- **contains_point(x, y)** - Hit testing
- Simple manual positioning (no auto-layout yet)

## Phase 3: Window Manager

### 3.1 Window Structure
```rust
pub struct Window {
    title: String,
    rect: Rect,
    widgets: Vec<Box<dyn Widget>>,
    focused: bool,
    dragging: bool,
    drag_offset: (i32, i32),
}
```

### 3.2 Window Features
- Title bar with close button
- Draggable by title bar
- Focus management (click to focus)
- Draw order (focused window on top)
- Border and shadow

### 3.3 Window Manager
```rust
pub struct WindowManager {
    windows: Vec<Window>,
    focused_window: Option<usize>,
}
```

## Phase 4: File Browser Application

### 4.1 File List Widget
- Display files from filesystem
- Scroll with mouse wheel (or buttons)
- Click to select file
- Double-click to open

### 4.2 File Viewer Window
- Open when file is double-clicked
- Display text content of file
- Close button
- Read-only for now

### 4.3 File Operations
- Delete button (confirm dialog?)
- Create new file dialog
- Basic file info display

## Phase 5: Text Editor (Stretch Goal)

### 5.1 Text Buffer
- Line-based storage
- Cursor position (line, column)
- Insert/delete characters
- Newline handling

### 5.2 Text Editing
- Keyboard input appends to cursor
- Backspace deletes before cursor
- Arrow keys move cursor
- Basic selection (shift + arrows)

### 5.3 File Operations
- Save buffer to file
- Load file into buffer
- "Dirty" flag for unsaved changes

## Implementation Order

1. **Start:** Drawing primitives in `graphics.rs`
2. **Test:** Draw colored rectangles and text on screen
3. **Next:** Create Button widget
4. **Test:** Click button to change color
5. **Next:** Create Window with title bar
6. **Test:** Drag window around screen
7. **Next:** File browser widget
8. **Test:** List files, click to select
9. **Next:** File viewer window
10. **Done:** Working GUI file browser!

## Quick Win: Hello Window

Minimal working example to start:
```rust
// In mod.rs after shell init:
let mut gui = Gui::new(framebuffer);
let mut window = Window::new("Hello World", 100, 100, 300, 200);
window.add_widget(Label::new(10, 10, "Welcome to rOSt!"));
window.add_widget(Button::new(10, 50, 100, 30, "Click Me"));
gui.add_window(window);
gui.run(); // Main event loop
```

## Technical Details

### Framebuffer Access
- Already have framebuffer from UEFI GOP
- Need to wrap it in safe interface
- Double buffering if needed for flicker-free

### Event Handling
- VirtIO input already provides mouse/keyboard events
- Convert to GUI events (Click, KeyPress, etc.)
- Route events to focused window → widget

### Performance
- Only redraw changed areas (dirty rectangles)
- Cache rendered text if needed
- Keep it simple initially - optimize later

## Resources
- [OSDev GUI](https://wiki.osdev.org/GUI)
- Embedded Rust GUI libraries for inspiration (but implement from scratch)
- Keep font simple: 8x8 or 8x16 bitmap font

---

**Status:** Ready to start Phase 1 - Drawing Primitives
**First Task:** Create `src/kernel/graphics.rs` with basic drawing functions
