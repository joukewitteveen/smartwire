pub trait Get<T> {
    fn get(&self) -> T;
}

#[cfg_attr(feature = "p1ep", path = "p1ep.rs")]
mod bsp;

pub use bsp::Board;
