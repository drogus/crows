pub mod crossterm;
pub mod simple;

// Re-export the printers for easier access
pub use crossterm::Printer as Crossterm;
pub use simple::Printer as Simple;
