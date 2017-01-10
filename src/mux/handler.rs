use Reset;
use error::*;
use handler::*;
use nix::sys::socket::*;
use epoll::*;
use slab::Slab;

use super::*;
use super::action::*;

#[derive(Debug)]
pub struct SyncMux<'m, H, P, R> {
  epfd: EpollFd,
  handlers: Slab<H, usize>,
  resources: Vec<R>,
  factory: P,
  interests: EpollEventKind,
  _marker: ::std::marker::PhantomData<&'m ()>,
}

impl<'m, H, P, R> SyncMux<'m, H, P, R>
  where H: Handler<MuxEvent<'m, R>, MuxCmd> + EpollHandler,
        P: HandlerFactory<'m, H, R> + 'm,
        R: Clone + 'm,
{
  pub fn new(max_handlers: usize, epfd: EpollFd, factory: P) -> SyncMux<'m, H, P, R> {
    SyncMux {
      epfd: epfd,
      handlers: Slab::with_capacity(max_handlers),
      resources: vec!(factory.new_resource(); max_handlers),
      factory: factory,
      interests: H::interests(),
      _marker: ::std::marker::PhantomData {},
    }
  }
}

macro_rules! close {
  ($clifd: expr) => {{
    if let Err(e) = eintr!(::unistd::close($clifd)) {
      report_err!(e.into());
    }
    return EpollCmd::Poll;
  }}
}

macro_rules! some {
  ($cmd:expr, $clifd: expr) => {{
    match $cmd {
      None => {
        return EpollCmd::Poll;
      },
      Some(res) => res,
    }
  }}
}

impl<'m, H, P, R> Handler<EpollEvent, EpollCmd> for SyncMux<'m, H, P, R>
  where H: Handler<MuxEvent<'m, R>, MuxCmd> + EpollHandler,
        P: HandlerFactory<'m, H, R> + 'm,
        R: Reset + Clone + 'm,
{
  fn on_next(&mut self, event: EpollEvent) -> EpollCmd {

    match Action::decode(event.data) {

      Action::Notify(i, clifd) => {
        let mut entry = some!(self.handlers.entry(i), clifd);
        let resource = unsafe { &mut *(&mut self.resources[i] as *mut R) };

        let mux_event = MuxEvent {
          resource: resource,
          kind: event.events,
          fd: clifd,
        };

        keep_or!(entry.get_mut().on_next(mux_event), {
          self.resources[i].reset();
          close!(clifd);
        });
      }

      Action::New(data) => {
        let srvfd = data as i32;
        match eintr!(accept(srvfd)) {
          Ok(Some(clifd)) => {
            debug!("accept: accepted new tcp client {}", &clifd);
            // TODO grow slab, deprecate max_conn in favour of reserve slots
            // or Mux::reserve to pre-allocate and then grow as it needs more
            let entry = some!(self.handlers.vacant_entry(), clifd);
            let i = entry.index();

            let h = self.factory.new_handler(self.epfd, clifd);

            let event = EpollEvent {
              events: self.interests,
              data: Action::encode(Action::Notify(i, clifd)),
            };

            self.epfd.register(clifd, &event).unwrap();

            entry.insert(h);
          }
          Ok(None) => debug!("accept4: socket not ready"),
          Err(e) => report_err!(e.into()),
        }
      }
    };

    EpollCmd::Poll
  }
}

impl<'m, H, P, R> EpollHandler for SyncMux<'m, H, P, R> {
  // TODO: check that linux >= 4.5 for EPOLLEXCLUSIVE
  fn interests() -> EpollEventKind {
    EPOLLIN | EPOLLEXCLUSIVE
  }

  fn with_epfd(&mut self, epfd: EpollFd) {
    self.epfd = epfd;
  }
}

impl<'m, H, P, R> Clone for SyncMux<'m, H, P, R>
  where H: Handler<MuxEvent<'m, R>, MuxCmd> + EpollHandler,
        P: HandlerFactory<'m, H, R> + Clone + 'm,
        R: Clone + 'm,
{
  fn clone(&self) -> Self {
    SyncMux {
      epfd: self.epfd,
      handlers: Slab::with_capacity(self.handlers.capacity()),
      resources: vec!(self.factory.new_resource(); self.handlers.capacity()),
      factory: self.factory.clone(),
      interests: self.interests.clone(),
      _marker: ::std::marker::PhantomData {},
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn should_grow_slab() {
    assert!(false);
  }
}