#[macro_use]
extern crate log;
#[macro_use]
extern crate rux;
extern crate num_cpus;
extern crate env_logger;

use rux::{RawFd, Reset};
use rux::buf::ByteBuffer;
use rux::handler::*;
use rux::mux::*;
use rux::epoll::*;
use rux::sys::socket::*;
use rux::prop::server::*;
use rux::daemon::*;

const BUF_SIZE: usize = 2048;
const EPOLL_BUF_CAP: usize = 2048;
const EPOLL_LOOP_MS: isize = -1;
const MAX_CONN: usize = 2048;

/// Handler that echoes incoming bytes
///
/// For benchmarking I/O throuput and latency
pub struct EchoHandler {
  closed: bool
}

impl<'a> Handler<MuxEvent<'a, ByteBuffer>, MuxCmd> for EchoHandler {

  fn next(&mut self) -> MuxCmd {
    if self.closed {
      return MuxCmd::Close;
    }

    MuxCmd::Keep
  }

  fn on_next(&mut self, event: MuxEvent<'a, ByteBuffer>) {

    let fd = event.fd;
    let events = event.events;
    let buffer = event.resource;

    if events.contains(EPOLLHUP) {
      trace!("socket's fd {}: EPOLLHUP", fd);
      self.closed = true;
      return;
    }

    if events.contains(EPOLLERR) {
      error!("socket's fd {}: EPOLERR", fd);
      self.closed = true;
      return;
    }

    if events.contains(EPOLLIN) {
      if let Some(n) = syscall!(recv(fd, From::from(&mut *buffer), MSG_DONTWAIT)).unwrap() {
        buffer.extend(n);
      }
    }

    if events.contains(EPOLLOUT) {
      if buffer.is_readable() {
        if let Some(cnt) = syscall!(send(fd, From::from(&*buffer), MSG_DONTWAIT)).unwrap() {
          buffer.consume(cnt);
        }
      }
    }
  }
}

impl EpollHandler for EchoHandler {
  fn interests() -> EpollEventKind {
    EPOLLIN | EPOLLOUT | EPOLLET
  }

  fn with_epfd(&mut self, _: EpollFd) {

  }
}

impl Reset for EchoHandler {
  fn reset(&mut self) {}
}

#[derive(Clone, Debug)]
struct EchoFactory;

impl<'a> HandlerFactory<'a, EchoHandler, ByteBuffer> for EchoFactory {

  fn new_resource(&self) -> ByteBuffer {
    ByteBuffer::with_capacity(BUF_SIZE)
  }

  fn new_handler(&mut self, _: EpollFd, _: RawFd) -> EchoHandler {
    EchoHandler { 
      closed: false
    }
  }
}

fn main() {

  ::env_logger::init().unwrap();

  info!("BUF_SIZE: {}; EPOLL_BUF_CAP: {}; EPOLL_LOOP_MS: {}; MAX_CONN: {}",
        BUF_SIZE,
        EPOLL_BUF_CAP,
        EPOLL_LOOP_MS,
        MAX_CONN);

  let config = ServerConfig::tcp(("127.0.0.1", 9999))
    .unwrap()
    .max_conn(MAX_CONN)
    .io_threads(1)
    // .io_threads(::std::cmp::max(1, ::num_cpus::get() / 2))
    .epoll_config(EpollConfig {
      loop_ms: EPOLL_LOOP_MS,
      buffer_capacity: EPOLL_BUF_CAP,
    });

  let server = Server::new(config, EchoFactory).unwrap();

  Daemon::build(server)
    .with_sched(SCHED_FIFO, None)
    .run().unwrap();
}
