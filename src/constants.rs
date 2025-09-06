//Number of slots that can have proposals pending
pub const WINDOW: u64 = 5;

// Multiplicative increase amount for liveness timeouts
pub const TIMEOUT_MULTIPLY: f32 = 1.2;

// Additive decrease amount for liveness timeouts
pub const TIMEOUT_SUBTRACT: f32 = 0.03;
