// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.
// Vendored from Deno v2.0.0 cli/napi/ and adapted for howth.

#![allow(unused_mut)]
#![allow(non_camel_case_types)]
#![allow(clippy::undocumented_unsafe_blocks)]

pub mod js_native_api;
pub mod node_api;
pub mod util;
pub mod uv;

/// Force LLVM to keep all N-API and libuv symbol implementations, even under
/// LTO.  `#[used]` prevents the optimizer from eliminating the static, and the
/// function-pointer entries prevent it from eliminating the referenced
/// functions.  Native addons (.node files) resolve these symbols at runtime via
/// dlsym.
///
/// We use `extern "C"` declarations for the uv symbols because their concrete
/// Rust types are private to the `uv` module.

extern "C" {
    fn uv_mutex_init();
    fn uv_mutex_lock();
    fn uv_mutex_unlock();
    fn uv_mutex_destroy();
    fn uv_async_init();
    fn uv_async_send();
    fn uv_close();
}

struct FnPtr(*const ());
unsafe impl Sync for FnPtr {}

macro_rules! keep {
    ($($sym:path),* $(,)?) => {
        [$(FnPtr($sym as *const ())),*]
    };
}

#[used]
static NAPI_SYMBOLS: [FnPtr; 160] = keep! {
    // js_native_api.rs symbols
    js_native_api::napi_get_last_error_info,
    js_native_api::napi_create_function,
    js_native_api::napi_define_class,
    js_native_api::napi_get_property_names,
    js_native_api::napi_get_all_property_names,
    js_native_api::napi_set_property,
    js_native_api::napi_has_property,
    js_native_api::napi_get_property,
    js_native_api::napi_delete_property,
    js_native_api::napi_has_own_property,
    js_native_api::napi_has_named_property,
    js_native_api::napi_set_named_property,
    js_native_api::napi_get_named_property,
    js_native_api::napi_set_element,
    js_native_api::napi_has_element,
    js_native_api::napi_get_element,
    js_native_api::napi_delete_element,
    js_native_api::napi_define_properties,
    js_native_api::napi_object_freeze,
    js_native_api::napi_object_seal,
    js_native_api::napi_is_array,
    js_native_api::napi_get_array_length,
    js_native_api::napi_strict_equals,
    js_native_api::napi_get_prototype,
    js_native_api::napi_create_object,
    js_native_api::napi_create_array,
    js_native_api::napi_create_array_with_length,
    js_native_api::napi_create_string_latin1,
    js_native_api::napi_create_string_utf8,
    js_native_api::napi_create_string_utf16,
    js_native_api::node_api_create_external_string_latin1,
    js_native_api::node_api_create_external_string_utf16,
    js_native_api::node_api_create_property_key_utf16,
    js_native_api::napi_create_double,
    js_native_api::napi_create_int32,
    js_native_api::napi_create_uint32,
    js_native_api::napi_create_int64,
    js_native_api::napi_create_bigint_int64,
    js_native_api::napi_create_bigint_uint64,
    js_native_api::napi_create_bigint_words,
    js_native_api::napi_get_boolean,
    js_native_api::napi_create_symbol,
    js_native_api::node_api_symbol_for,
    js_native_api::napi_create_error,
    js_native_api::napi_create_type_error,
    js_native_api::napi_create_range_error,
    js_native_api::node_api_create_syntax_error,
    js_native_api::napi_typeof,
    js_native_api::napi_get_undefined,
    js_native_api::napi_get_null,
    js_native_api::napi_get_cb_info,
    js_native_api::napi_get_new_target,
    js_native_api::napi_call_function,
    js_native_api::napi_get_global,
    js_native_api::napi_throw,
    js_native_api::napi_throw_error,
    js_native_api::napi_throw_type_error,
    js_native_api::napi_throw_range_error,
    js_native_api::node_api_throw_syntax_error,
    js_native_api::napi_is_error,
    js_native_api::napi_get_value_double,
    js_native_api::napi_get_value_int32,
    js_native_api::napi_get_value_uint32,
    js_native_api::napi_get_value_int64,
    js_native_api::napi_get_value_bigint_int64,
    js_native_api::napi_get_value_bigint_uint64,
    js_native_api::napi_get_value_bigint_words,
    js_native_api::napi_get_value_bool,
    js_native_api::napi_get_value_string_latin1,
    js_native_api::napi_get_value_string_utf8,
    js_native_api::napi_get_value_string_utf16,
    js_native_api::napi_coerce_to_bool,
    js_native_api::napi_coerce_to_number,
    js_native_api::napi_coerce_to_object,
    js_native_api::napi_coerce_to_string,
    js_native_api::napi_wrap,
    js_native_api::napi_unwrap,
    js_native_api::napi_remove_wrap,
    js_native_api::napi_create_external,
    js_native_api::napi_type_tag_object,
    js_native_api::napi_check_object_type_tag,
    js_native_api::napi_get_value_external,
    js_native_api::napi_create_reference,
    js_native_api::napi_delete_reference,
    js_native_api::napi_reference_ref,
    js_native_api::napi_reference_unref,
    js_native_api::napi_get_reference_value,
    js_native_api::napi_open_handle_scope,
    js_native_api::napi_close_handle_scope,
    js_native_api::napi_open_escapable_handle_scope,
    js_native_api::napi_close_escapable_handle_scope,
    js_native_api::napi_escape_handle,
    js_native_api::napi_new_instance,
    js_native_api::napi_instanceof,
    js_native_api::napi_is_exception_pending,
    js_native_api::napi_get_and_clear_last_exception,
    js_native_api::napi_is_arraybuffer,
    js_native_api::napi_create_arraybuffer,
    js_native_api::napi_create_external_arraybuffer,
    js_native_api::napi_get_arraybuffer_info,
    js_native_api::napi_is_typedarray,
    js_native_api::napi_create_typedarray,
    js_native_api::napi_get_typedarray_info,
    js_native_api::napi_create_dataview,
    js_native_api::napi_is_dataview,
    js_native_api::napi_get_dataview_info,
    js_native_api::napi_get_version,
    js_native_api::napi_create_promise,
    js_native_api::napi_resolve_deferred,
    js_native_api::napi_reject_deferred,
    js_native_api::napi_is_promise,
    js_native_api::napi_create_date,
    js_native_api::napi_is_date,
    js_native_api::napi_get_date_value,
    js_native_api::napi_run_script,
    js_native_api::napi_add_finalizer,
    js_native_api::node_api_post_finalizer,
    js_native_api::napi_adjust_external_memory,
    js_native_api::napi_set_instance_data,
    js_native_api::napi_get_instance_data,
    js_native_api::napi_detach_arraybuffer,
    js_native_api::napi_is_detached_arraybuffer,
    // node_api.rs symbols
    node_api::napi_module_register,
    node_api::napi_add_env_cleanup_hook,
    node_api::napi_remove_env_cleanup_hook,
    node_api::napi_add_async_cleanup_hook,
    node_api::napi_remove_async_cleanup_hook,
    node_api::napi_fatal_exception,
    node_api::napi_fatal_error,
    node_api::napi_open_callback_scope,
    node_api::napi_close_callback_scope,
    node_api::napi_async_init,
    node_api::napi_async_destroy,
    node_api::napi_make_callback,
    node_api::napi_create_buffer,
    node_api::napi_create_external_buffer,
    node_api::napi_create_buffer_copy,
    node_api::napi_is_buffer,
    node_api::napi_get_buffer_info,
    node_api::napi_get_node_version,
    node_api::napi_create_async_work,
    node_api::napi_delete_async_work,
    node_api::napi_get_uv_event_loop,
    node_api::napi_queue_async_work,
    node_api::napi_cancel_async_work,
    node_api::napi_create_threadsafe_function,
    node_api::napi_get_threadsafe_function_context,
    node_api::napi_call_threadsafe_function,
    node_api::napi_acquire_threadsafe_function,
    node_api::napi_release_threadsafe_function,
    node_api::napi_unref_threadsafe_function,
    node_api::napi_ref_threadsafe_function,
    node_api::node_api_get_module_file_name,
    // uv.rs symbols
    uv_mutex_init,
    uv_mutex_lock,
    uv_mutex_unlock,
    uv_mutex_destroy,
    uv_async_init,
    uv_async_send,
    uv_close,
};
