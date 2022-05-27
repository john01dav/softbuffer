use crate::{error::unwrap, GraphicsContextImpl, SoftBufferError};
use raw_window_handle::{HasRawWindowHandle, WaylandHandle};
use std::{
    fs::File,
    io::Write,
    os::unix::prelude::{AsRawFd, FileExt},
};
use tempfile::tempfile;
use wayland_client::{
    protocol::{wl_buffer::WlBuffer, wl_shm::WlShm, wl_surface::WlSurface},
    sys::client::wl_display,
    Display, EventQueue, GlobalManager, Main, Proxy,
};

pub struct WaylandImpl {
    _event_queue: EventQueue,
    surface: WlSurface,
    shm: Main<WlShm>,
    tempfile: File,
    buffer: Option<WaylandBuffer>,
}

struct WaylandBuffer {
    width: i32,
    height: i32,
    buffer: Main<WlBuffer>,
}

impl WaylandImpl {
    pub unsafe fn new<W: HasRawWindowHandle>(
        handle: WaylandHandle,
    ) -> Result<Self, SoftBufferError<W>> {
        let display = Display::from_external_display(handle.display as *mut wl_display);
        let mut event_queue = display.create_event_queue();
        let attached_display = (*display).clone().attach(event_queue.token());
        let globals = GlobalManager::new(&attached_display);
        unwrap(
            event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!()),
            "Failed to make round trip to server",
        )?;
        let shm = unwrap(
            globals.instantiate_exact::<WlShm>(1),
            "Failed to instantiate Wayland Shm",
        )?;
        let tempfile = unwrap(
            tempfile(),
            "Failed to create temporary file to store buffer.",
        )?;
        let surface = Proxy::from_c_ptr(handle.surface as _).into();
        Ok(Self {
            _event_queue: event_queue,
            surface,
            shm,
            tempfile,
            buffer: None,
        })
    }

    fn ensure_buffer_size(&mut self, width: i32, height: i32) {
        if !self.check_buffer_size_equals(width, height) {
            let pool = self
                .shm
                .create_pool(self.tempfile.as_raw_fd(), width * height * 4);
            let buffer = pool.create_buffer(
                0,
                width,
                height,
                width * 4,
                wayland_client::protocol::wl_shm::Format::Xrgb8888,
            );
            self.buffer = Some(WaylandBuffer {
                width,
                height,
                buffer,
            });
        }
    }

    fn check_buffer_size_equals(&self, width: i32, height: i32) -> bool {
        match &self.buffer {
            Some(buffer) => buffer.width == width && buffer.height == height,
            None => false,
        }
    }
}

impl GraphicsContextImpl for WaylandImpl {
    unsafe fn set_buffer(&mut self, buffer: &[u32], width: u16, height: u16) {
        self.ensure_buffer_size(width as i32, height as i32);
        let wayland_buffer = self.buffer.as_mut().unwrap();
        self.tempfile
            .write_at(
                std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len() * 4),
                0,
            )
            .expect("Failed to write buffer to temporary file.");
        self.tempfile
            .flush()
            .expect("Failed to flush buffer to temporary file.");
        self.surface.attach(Some(&wayland_buffer.buffer), 0, 0);
        self.surface.commit();
    }
}
