// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.
// Vendored from Deno v2.0.0 cli/napi/uv.rs and adapted for howth.

use deno_core::parking_lot::Mutex;
use ::deno_napi::*;
use std::mem::MaybeUninit;
use std::ptr::addr_of_mut;

#[allow(clippy::print_stderr)]
fn assert_ok(res: c_int) -> c_int {
  if res != 0 {
    eprintln!("bad result in uv polyfill: {res}");
    std::process::abort();
  }
  res
}

use crate::napi::js_native_api::napi_create_string_utf8;
use crate::napi::node_api::napi_create_async_work;
use crate::napi::node_api::napi_delete_async_work;
use crate::napi::node_api::napi_queue_async_work;
use std::ffi::c_int;

const UV_MUTEX_SIZE: usize = {
  #[cfg(unix)]
  {
    std::mem::size_of::<libc::pthread_mutex_t>()
  }
  #[cfg(windows)]
  {
    // windows_sys CRITICAL_SECTION size
    40
  }
};

const UV_MUTEX_PADDING: usize =
  (UV_MUTEX_SIZE - std::mem::size_of::<Mutex<()>>()) / std::mem::size_of::<usize>();

#[repr(C)]
struct uv_mutex_t {
  mutex: Mutex<()>,
  _padding: [MaybeUninit<usize>; UV_MUTEX_PADDING],
}

#[no_mangle]
unsafe extern "C" fn uv_mutex_init(lock: *mut uv_mutex_t) -> c_int {
  unsafe {
    addr_of_mut!((*lock).mutex).write(Mutex::new(()));
    0
  }
}

#[no_mangle]
unsafe extern "C" fn uv_mutex_lock(lock: *mut uv_mutex_t) {
  unsafe {
    let guard = (*lock).mutex.lock();
    std::mem::forget(guard);
  }
}

#[no_mangle]
unsafe extern "C" fn uv_mutex_unlock(lock: *mut uv_mutex_t) {
  unsafe {
    (*lock).mutex.force_unlock();
  }
}

#[no_mangle]
unsafe extern "C" fn uv_mutex_destroy(_lock: *mut uv_mutex_t) {
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum uv_handle_type {
  UV_UNKNOWN_HANDLE = 0,
  UV_ASYNC,
  UV_CHECK,
  UV_FS_EVENT,
  UV_FS_POLL,
  UV_HANDLE,
  UV_IDLE,
  UV_NAMED_PIPE,
  UV_POLL,
  UV_PREPARE,
  UV_PROCESS,
  UV_STREAM,
  UV_TCP,
  UV_TIMER,
  UV_TTY,
  UV_UDP,
  UV_SIGNAL,
  UV_FILE,
  UV_HANDLE_TYPE_MAX,
}

const UV_HANDLE_SIZE: usize = 96;

const UV_HANDLE_PADDING: usize =
  (UV_HANDLE_SIZE
    - std::mem::size_of::<*mut c_void>()
    - std::mem::size_of::<*mut usize>() // uv_loop_t ptr
    - std::mem::size_of::<uv_handle_type>())
    / std::mem::size_of::<usize>();

#[repr(C)]
struct uv_handle_t {
  pub data: *mut c_void,
  pub r#loop: *mut uv_loop_t,
  pub r#type: uv_handle_type,
  _padding: [MaybeUninit<usize>; UV_HANDLE_PADDING],
}

#[cfg(unix)]
const UV_ASYNC_SIZE: usize = 128;

#[cfg(windows)]
const UV_ASYNC_SIZE: usize = 224;

const UV_ASYNC_PADDING: usize =
  (UV_ASYNC_SIZE
    - std::mem::size_of::<*mut c_void>()
    - std::mem::size_of::<*mut usize>() // uv_loop_t ptr
    - std::mem::size_of::<uv_handle_type>()
    - std::mem::size_of::<uv_async_cb>()
    - std::mem::size_of::<napi_async_work>())
    / std::mem::size_of::<usize>();

#[repr(C)]
struct uv_async_t {
  pub data: *mut c_void,
  pub r#loop: *mut uv_loop_t,
  pub r#type: uv_handle_type,
  async_cb: uv_async_cb,
  work: napi_async_work,
  _padding: [MaybeUninit<usize>; UV_ASYNC_PADDING],
}

type uv_loop_t = Env;
type uv_async_cb = extern "C" fn(handle: *mut uv_async_t);

#[no_mangle]
unsafe extern "C" fn uv_async_init(
  r#loop: *mut uv_loop_t,
  r#async: *mut uv_async_t,
  async_cb: uv_async_cb,
) -> c_int {
  unsafe {
    addr_of_mut!((*r#async).r#loop).write(r#loop);
    addr_of_mut!((*r#async).r#type).write(uv_handle_type::UV_ASYNC);
    addr_of_mut!((*r#async).async_cb).write(async_cb);

    let mut resource_name: MaybeUninit<napi_value> = MaybeUninit::uninit();
    assert_ok(napi_create_string_utf8(
      r#loop,
      c"uv_async".as_ptr(),
      usize::MAX,
      resource_name.as_mut_ptr(),
    ));
    let resource_name = resource_name.assume_init();

    let res = napi_create_async_work(
      r#loop,
      None::<v8::Local<'static, v8::Value>>.into(),
      resource_name,
      Some(async_exec_wrap),
      None,
      r#async.cast(),
      addr_of_mut!((*r#async).work),
    );
    -res
  }
}

#[no_mangle]
unsafe extern "C" fn uv_async_send(handle: *mut uv_async_t) -> c_int {
  unsafe { -napi_queue_async_work((*handle).r#loop, (*handle).work) }
}

type uv_close_cb = unsafe extern "C" fn(*mut uv_handle_t);

#[no_mangle]
unsafe extern "C" fn uv_close(handle: *mut uv_handle_t, close: uv_close_cb) {
  unsafe {
    if handle.is_null() {
      close(handle);
      return;
    }
    if let uv_handle_type::UV_ASYNC = (*handle).r#type {
      let handle: *mut uv_async_t = handle.cast();
      napi_delete_async_work((*handle).r#loop, (*handle).work);
    }
    close(handle);
  }
}

unsafe extern "C" fn async_exec_wrap(_env: napi_env, data: *mut c_void) {
  let data: *mut uv_async_t = data.cast();
  unsafe {
    ((*data).async_cb)(data);
  }
}
