use std::{
    collections::HashMap,
    mem,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
};

use imgui::Context;
use once_cell::sync::{Lazy, OnceCell};
use parking_lot::Mutex;
use windows::{
    core::{Error, Result, HRESULT},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{
            CallWindowProcW,
            DefWindowProcW,
            SetWindowLongPtrA,
            GWLP_WNDPROC,
            WM_SIZE,
        },
    },
};

use crate::{renderer::RenderEngine, util, ImguiRenderLoop};

type RenderLoop = Box<dyn ImguiRenderLoop + Send + Sync>;

pub type WndProcType =
    unsafe extern "system" fn(hwnd: HWND, umsg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;

static mut PIPELINE_STATES: Lazy<Mutex<HashMap<isize, Arc<PipelineSharedState>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub(crate) struct PipelineMessage(pub(crate) u32, pub(crate) LPARAM);

pub(crate) struct PipelineSharedState {
    pub(crate) wnd_proc: WndProcType,
    pub(crate) tx: Sender<PipelineMessage>,
}

pub(crate) struct Pipeline<T: RenderEngine> {
    hwnd: HWND,
    ctx: Context,
    engine: T,
    render_loop: RenderLoop,
    rx: Receiver<PipelineMessage>,
    shared_state: Arc<PipelineSharedState>,
    queue_buffer: OnceCell<Vec<PipelineMessage>>,
}

impl<T: RenderEngine> Pipeline<T> {
    pub(crate) fn new(
        hwnd: HWND,
        mut ctx: Context,
        mut engine: T,
        mut render_loop: RenderLoop,
    ) -> std::result::Result<Self, (Error, RenderLoop)> {
        let (width, height) = util::win_size(hwnd);

        ctx.io_mut().display_size = [width as f32, height as f32];

        unsafe { render_loop.initialize(&mut ctx, &mut engine) };

        if let Err(e) = engine.setup_fonts(&mut ctx) {
            return Err((e, render_loop));
        }

        let wnd_proc = unsafe {
            #[cfg(target_arch = "x86")]
            type SwlpRet = i32;
            #[cfg(target_arch = "x86_64")]
            type SwlpRet = isize;

            mem::transmute::<SwlpRet, WndProcType>(SetWindowLongPtrA(
                hwnd,
                GWLP_WNDPROC,
                pipeline_wnd_proc as usize as _,
            ))
        };

        let (tx, rx) = mpsc::channel();
        let shared_state = Arc::new(PipelineSharedState {
            wnd_proc,
            tx,
        });

        unsafe { PIPELINE_STATES.lock() }.insert(hwnd.0, Arc::clone(&shared_state));

        let queue_buffer = OnceCell::from(Vec::new());

        Ok(Self {
            hwnd,
            ctx,
            engine,
            render_loop,
            rx,
            shared_state: Arc::clone(&shared_state),
            queue_buffer,
        })
    }

    pub(crate) fn prepare_render(&mut self) -> Result<()> {
        let mut queue_buffer = self.queue_buffer.take().unwrap();
        queue_buffer.clear();
        queue_buffer.extend(self.rx.try_iter());
        queue_buffer
            .drain(..)
            .for_each(|PipelineMessage(umsg, lparam)| {
                match umsg {
                    WM_SIZE => {
                        self.ctx.io_mut().display_size = [
                            (lparam.0 & 0xFFFF) as u16 as f32,
                            ((lparam.0 >> 16) & 0xFFFF) as u16 as f32,
                        ];
                    }
                    _ => {}
                };
            });

        self.queue_buffer
            .set(queue_buffer)
            .expect("OnceCell should be empty");

        let io = self.ctx.io_mut();

        io.nav_active = true;
        io.nav_visible = true;

        unsafe {
            self.render_loop
                .before_render(&mut self.ctx, &mut self.engine)
        };

        Ok(())
    }

    pub(crate) fn render(&mut self, render_target: T::RenderTarget) -> Result<()> {
        let [w, h] = self.ctx.io().display_size;
        let [fsw, fsh] = self.ctx.io().display_framebuffer_scale;

        if (w * fsw) <= 0.0 || (h * fsh) <= 0.0 {
            return Err(Error::from_hresult(HRESULT(-1)));
        }

        let ui = self.ctx.frame();
        unsafe { self.render_loop.render(ui) };
        let draw_data = self.ctx.render();

        self.engine.render(draw_data, render_target)?;

        Ok(())
    }

    pub(crate) fn cleanup(&mut self) {
        unsafe {
            SetWindowLongPtrA(
                self.hwnd,
                GWLP_WNDPROC,
                self.shared_state.wnd_proc as usize as _,
            )
        };
    }

    pub(crate) fn take(mut self) -> RenderLoop {
        self.cleanup();
        self.render_loop
    }
}

unsafe extern "system" fn pipeline_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let shared_state = {
        let Some(shared_state_guard) = PIPELINE_STATES.try_lock() else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };

        let Some(shared_state) = shared_state_guard.get(&hwnd.0) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };

        Arc::clone(shared_state)
    };

    let _ = shared_state.tx.send(PipelineMessage(msg, lparam));

    CallWindowProcW(Some(shared_state.wnd_proc), hwnd, msg, wparam, lparam)
}
