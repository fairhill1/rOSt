use alloc::vec::Vec;
use crate::kernel::timer::get_time_us;
extern crate alloc;

const GRID_SIZE: usize = 20; // 20x20 grid
const CELL_SIZE: usize = 20; // Each cell is 20x20 pixels
const GAME_WIDTH: usize = GRID_SIZE * CELL_SIZE;
const GAME_HEIGHT: usize = GRID_SIZE * CELL_SIZE;
const UPDATE_INTERVAL_US: u64 = 150_000; // Update every 150ms

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Position {
    x: i32,
    y: i32,
}

pub struct SnakeGame {
    snake: Vec<Position>,
    direction: Direction,
    next_direction: Direction,
    food: Position,
    game_over: bool,
    score: u32,
    last_update: u64,
    // Random seed for food generation
    rng_state: u64,
}

impl SnakeGame {
    pub fn new() -> Self {
        let mut game = SnakeGame {
            snake: Vec::new(),
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: Position { x: 0, y: 0 },
            game_over: false,
            score: 0,
            last_update: get_time_us(),
            rng_state: get_time_us(),
        };

        // Initialize snake in the middle
        game.snake.push(Position { x: 10, y: 10 });
        game.snake.push(Position { x: 9, y: 10 });
        game.snake.push(Position { x: 8, y: 10 });

        game.spawn_food();
        game
    }

    pub fn reset(&mut self) {
        self.snake.clear();
        self.snake.push(Position { x: 10, y: 10 });
        self.snake.push(Position { x: 9, y: 10 });
        self.snake.push(Position { x: 8, y: 10 });
        self.direction = Direction::Right;
        self.next_direction = Direction::Right;
        self.game_over = false;
        self.score = 0;
        self.last_update = get_time_us();
        self.spawn_food();
    }

    pub fn set_direction(&mut self, dir: Direction) {
        // Prevent 180-degree turns
        let can_turn = match (self.direction, dir) {
            (Direction::Up, Direction::Down) => false,
            (Direction::Down, Direction::Up) => false,
            (Direction::Left, Direction::Right) => false,
            (Direction::Right, Direction::Left) => false,
            _ => true,
        };

        if can_turn {
            self.next_direction = dir;
        }
    }

    pub fn update(&mut self) -> bool {
        if self.game_over {
            return false;
        }

        let now = get_time_us();
        if now - self.last_update < UPDATE_INTERVAL_US {
            return false;
        }
        self.last_update = now;

        // Update direction
        self.direction = self.next_direction;

        // Calculate new head position
        let head = self.snake[0];
        let new_head = match self.direction {
            Direction::Up => Position { x: head.x, y: head.y - 1 },
            Direction::Down => Position { x: head.x, y: head.y + 1 },
            Direction::Left => Position { x: head.x - 1, y: head.y },
            Direction::Right => Position { x: head.x + 1, y: head.y },
        };

        // Check wall collision
        if new_head.x < 0 || new_head.x >= GRID_SIZE as i32 ||
           new_head.y < 0 || new_head.y >= GRID_SIZE as i32 {
            self.game_over = true;
            return true; // State changed
        }

        // Check self collision
        if self.snake.contains(&new_head) {
            self.game_over = true;
            return true; // State changed
        }

        // Add new head
        self.snake.insert(0, new_head);

        // Check if we ate food
        if new_head == self.food {
            self.score += 10;
            self.spawn_food();
        } else {
            // Remove tail
            self.snake.pop();
        }

        true // State changed
    }

    fn spawn_food(&mut self) {
        // Simple LCG random number generator
        loop {
            self.rng_state = self.rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let x = ((self.rng_state >> 16) % GRID_SIZE as u64) as i32;

            self.rng_state = self.rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let y = ((self.rng_state >> 16) % GRID_SIZE as u64) as i32;

            let pos = Position { x, y };

            // Make sure food doesn't spawn on snake
            if !self.snake.contains(&pos) {
                self.food = pos;
                break;
            }
        }
    }

    pub fn render(&self, fb: &mut [u32], width: usize, height: usize, x_offset: usize, y_offset: usize) {
        // Draw background (dark gray)
        for y in 0..GAME_HEIGHT.min(height) {
            for x in 0..GAME_WIDTH.min(width) {
                let px = x_offset + x;
                let py = y_offset + y;
                if px < width && py < height {
                    fb[py * width + px] = 0xFF1A1A1A;
                }
            }
        }

        // Draw grid lines (slightly lighter gray)
        for i in 0..=GRID_SIZE {
            let pos = i * CELL_SIZE;
            // Vertical lines
            if pos < GAME_WIDTH {
                for y in 0..GAME_HEIGHT.min(height) {
                    let px = x_offset + pos;
                    let py = y_offset + y;
                    if px < width && py < height {
                        fb[py * width + px] = 0xFF2A2A2A;
                    }
                }
            }
            // Horizontal lines
            if pos < GAME_HEIGHT {
                for x in 0..GAME_WIDTH.min(width) {
                    let px = x_offset + x;
                    let py = y_offset + pos;
                    if px < width && py < height {
                        fb[py * width + px] = 0xFF2A2A2A;
                    }
                }
            }
        }

        // Draw food (red)
        self.draw_cell(fb, width, height, x_offset, y_offset, self.food, 0xFFFF0000);

        // Draw snake (green for head, lighter green for body)
        for (i, segment) in self.snake.iter().enumerate() {
            let color = if i == 0 {
                0xFF00FF00 // Bright green for head
            } else {
                0xFF00AA00 // Darker green for body
            };
            self.draw_cell(fb, width, height, x_offset, y_offset, *segment, color);
        }

        // Draw score
        let score_text = alloc::format!("Score: {}", self.score);
        self.draw_text(fb, width, height, x_offset + 5, y_offset + GAME_HEIGHT + 10, &score_text, 0xFFFFFFFF);

        // Draw game over message
        if self.game_over {
            let msg = "GAME OVER! Press R to restart";
            let msg_width = msg.len() * 8;
            let msg_x = x_offset + (GAME_WIDTH / 2).saturating_sub(msg_width / 2);
            let msg_y = y_offset + (GAME_HEIGHT / 2);

            // Draw background for text
            for dy in 0..20 {
                for dx in 0..msg_width + 10 {
                    let px = msg_x.saturating_sub(5) + dx;
                    let py = msg_y.saturating_sub(5) + dy;
                    if px < width && py < height {
                        fb[py * width + px] = 0xFF000000;
                    }
                }
            }

            self.draw_text(fb, width, height, msg_x, msg_y, msg, 0xFFFFFFFF);
        }
    }

    fn draw_cell(&self, fb: &mut [u32], width: usize, height: usize, x_offset: usize, y_offset: usize, pos: Position, color: u32) {
        let start_x = pos.x as usize * CELL_SIZE + 1; // +1 to avoid grid lines
        let start_y = pos.y as usize * CELL_SIZE + 1;
        let end_x = start_x + CELL_SIZE - 2; // -2 to avoid grid lines on both sides
        let end_y = start_y + CELL_SIZE - 2;

        for y in start_y..end_y {
            for x in start_x..end_x {
                if x < GAME_WIDTH && y < GAME_HEIGHT {
                    let px = x_offset + x;
                    let py = y_offset + y;
                    if px < width && py < height {
                        fb[py * width + px] = color;
                    }
                }
            }
        }
    }

    fn draw_text(&self, fb: &mut [u32], width: usize, height: usize, x: usize, y: usize, text: &str, color: u32) {
        // Simple 8x8 bitmap font rendering
        for (i, ch) in text.chars().enumerate() {
            let char_x = x + i * 8;
            if char_x >= width {
                break;
            }

            for dy in 0..8 {
                let py = y + dy;
                if py >= height {
                    break;
                }

                let pattern = get_char_pattern(ch, dy);
                for dx in 0..8 {
                    let px = char_x + dx;
                    if px < width && (pattern & (1 << (7 - dx))) != 0 {
                        fb[py * width + px] = color;
                    }
                }
            }
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    pub fn width(&self) -> usize {
        GAME_WIDTH
    }

    pub fn height(&self) -> usize {
        GAME_HEIGHT + 30 // Extra space for score
    }
}

// Simple 8x8 bitmap font (only characters we need)
fn get_char_pattern(ch: char, row: usize) -> u8 {
    match ch {
        '0' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00][row],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00][row],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00][row],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00][row],
        '4' => [0x0C, 0x1C, 0x2C, 0x4C, 0x7E, 0x0C, 0x0C, 0x00][row],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00][row],
        '6' => [0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00][row],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00][row],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00][row],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00][row],
        'A' => [0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00][row],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00][row],
        'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00][row],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00][row],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00][row],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00][row],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00][row],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x63, 0x00][row],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00][row],
        'V' => [0x42, 0x42, 0x42, 0x42, 0x42, 0x24, 0x18, 0x00][row],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00][row],
        'c' => [0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00][row],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00][row],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00][row],
        'r' => [0x00, 0x00, 0x5C, 0x66, 0x60, 0x60, 0x60, 0x00][row],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00][row],
        't' => [0x18, 0x18, 0x7E, 0x18, 0x18, 0x18, 0x0E, 0x00][row],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00][row],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00][row],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00][row],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00][row],
    }
}

// Static storage for multiple snake game instances
static mut SNAKE_GAMES: Option<alloc::vec::Vec<SnakeGame>> = None;
static mut NEXT_SNAKE_ID: usize = 0;

pub fn init() {
    unsafe {
        SNAKE_GAMES = Some(Vec::new());
        NEXT_SNAKE_ID = 0;
    }
}

pub fn create_snake_game() -> usize {
    unsafe {
        if SNAKE_GAMES.is_none() {
            init();
        }

        let id = NEXT_SNAKE_ID;
        NEXT_SNAKE_ID += 1;

        if let Some(ref mut games) = SNAKE_GAMES {
            games.push(SnakeGame::new());
        }

        id
    }
}

pub fn remove_snake_game(id: usize) {
    unsafe {
        if let Some(ref mut games) = SNAKE_GAMES {
            if id < games.len() {
                games.remove(id);
            }
        }
    }
}

pub fn get_snake_game(id: usize) -> Option<&'static mut SnakeGame> {
    unsafe {
        if let Some(ref mut games) = SNAKE_GAMES {
            games.get_mut(id)
        } else {
            None
        }
    }
}

pub fn update_all_games() -> bool {
    unsafe {
        if let Some(ref mut games) = SNAKE_GAMES {
            let mut any_changed = false;
            for game in games.iter_mut() {
                if game.update() {
                    any_changed = true;
                }
            }
            any_changed
        } else {
            false
        }
    }
}
