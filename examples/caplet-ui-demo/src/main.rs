//! CapletRT UI Demo
//!
//! A demonstration of embedded graphics concepts on CapletRT.
//! This example shows how to create a simple UI with buttons,
//! labels, and progress indicators.
//!
//! This runs on the host machine for demonstration purposes.

use std::string::String;

/// Simple button state
#[derive(Debug, Clone, Copy, PartialEq)]
enum ButtonState {
    Released,
    Pressed,
}

/// A simple button widget
struct Button {
    _x: i32,
    _y: i32,
    _width: u32,
    _height: u32,
    label: &'static str,
    state: ButtonState,
}

impl Button {
    /// Create a new button
    #[allow(dead_code)]
    pub fn new(x: i32, y: i32, width: u32, height: u32, label: &'static str) -> Self {
        Self {
            _x: x,
            _y: y,
            _width: width,
            _height: height,
            label,
            state: ButtonState::Released,
        }
    }

    /// Check if a point is inside the button
    #[allow(dead_code)]
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self._x
            && px < (self._x + self._width as i32)
            && py >= self._y
            && py < (self._y + self._height as i32)
    }

    /// Simulate pressing the button
    pub fn press(&mut self) {
        self.state = ButtonState::Pressed;
        println!("[UI] Button '{}' pressed", self.label);
    }

    /// Simulate releasing the button
    pub fn release(&mut self) {
        if self.state == ButtonState::Pressed {
            println!("[UI] Button '{}' released", self.label);
        }
        self.state = ButtonState::Released;
    }
}

/// Progress bar widget
struct ProgressBar {
    value: f32,
    max_value: f32,
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new(max_value: f32) -> Self {
        Self {
            value: 0.0,
            max_value,
        }
    }

    /// Set the progress value (0.0 to max_value)
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, self.max_value);
    }

    /// Get the progress percentage (0.0 to 100.0)
    pub fn percentage(&self) -> f32 {
        if self.max_value > 0.0 {
            (self.value / self.max_value) * 100.0
        } else {
            0.0
        }
    }
}

/// UI state managed by the application
struct UiState {
    buttons: [Button; 3],
    progress_bar: ProgressBar,
    counter: u32,
    selected_button: Option<usize>,
}

impl UiState {
    /// Create new UI state
    pub fn new() -> Self {
        Self {
            buttons: [
                Button::new(10, 10, 100, 40, "START"),
                Button::new(120, 10, 100, 40, "STOP"),
                Button::new(230, 10, 100, 40, "RESET"),
            ],
            progress_bar: ProgressBar::new(100.0),
            counter: 0,
            selected_button: None,
        }
    }

    /// Handle touch input (simulated)
    pub fn handle_touch(&mut self, x: i32, y: i32) {
        for (i, button) in self.buttons.iter_mut().enumerate() {
            if button.contains(x, y) {
                button.press();
                self.selected_button = Some(i);
                self.on_button_pressed(i);
                return;
            }
        }
    }

    /// Handle touch release
    pub fn handle_release(&mut self) {
        if let Some(idx) = self.selected_button {
            self.buttons[idx].release();
            self.selected_button = None;
        }
    }

    /// Called when a button is pressed
    fn on_button_pressed(&mut self, button_idx: usize) {
        match button_idx {
            0 => {
                // START
                println!("[UI] Starting counter...");
            }
            1 => {
                // STOP
                println!("[UI] Stopping counter...");
            }
            2 => {
                // RESET
                self.counter = 0;
                self.progress_bar.set_value(0.0);
                println!("[UI] Counter reset to 0");
            }
            _ => {}
        }
    }

    /// Update the UI state (called periodically)
    pub fn update(&mut self) {
        // Simulate counter increment
        if self.counter < 100 {
            self.counter += 1;
            self.progress_bar.set_value(self.counter as f32);
        }
    }
}

/// Render the UI to a string (for terminal output)
fn render_ui(state: &UiState) -> String {
    let progress_pct = state.progress_bar.percentage() as u32;
    let bar_len = ((state.progress_bar.value / state.progress_bar.max_value) * 20.0) as usize;

    let mut output = String::new();
    output.push_str("┌─────────────────────────────────────────────┐\n");
    output.push_str("│           CapletRT UI Demo                 │\n");
    output.push_str("├─────────────────────────────────────────────┤\n");
    output.push_str("│  [START]   [STOP]    [RESET]              │\n");
    output.push_str("│                                             │\n");
    output.push_str(&format!("│  Progress: [{}{}] {}%\n",
        "█".repeat(bar_len),
        "░".repeat(20 - bar_len),
        progress_pct));
    output.push_str(&format!("│  Counter:  {} / 100                      │\n", state.counter));
    output.push_str("│                                             │\n");

    // Draw button states
    for (i, btn) in state.buttons.iter().enumerate() {
        let state_char = if btn.state == ButtonState::Pressed { "[X]" } else { "[ ]" };
        output.push_str(&format!("│  Button {}: {} {}                     │\n",
            i, state_char, btn.label));
    }

    output.push_str("└─────────────────────────────────────────────┘\n");
    output
}

/// Main UI demo
fn ui_demo() {
    println!("[UI] UI Demo started");

    // Simulate a few frames
    for frame in 0..10 {
        let mut s = UiState::new();
        s.update();

        println!("[UI] Frame {}:", frame);
        println!("{}", render_ui(&s));

        // Simulate button interactions
        match frame {
            2 => {
                // Press START
                s.handle_touch(60, 30); // Center of START button
                s.handle_release();
            }
            5 => {
                // Press RESET
                s.handle_touch(280, 30); // Center of RESET button
                s.handle_release();
            }
            _ => {}
        }
    }

    println!("[UI] UI Demo completed");
}

/// Host entry point
fn main() {
    println!("==============================================");
    println!("  CapletRT UI Demo");
    println!("  Press buttons to interact");
    println!("==============================================\n");

    ui_demo();

    println!("\n==============================================");
    println!("  Demo completed!");
    println!("==============================================");
}
