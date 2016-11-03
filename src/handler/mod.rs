pub mod echo;
pub mod sync;

// Epoll Events handler.
pub trait Handler<E> {
    fn is_terminated(&self) -> bool;
    fn ready(&mut self, e: &E);
}

// TODO Bind, Ready, Accept, Loop (is_terminated) traits
// pub trait Bind {
// 
//     fn 
// }
