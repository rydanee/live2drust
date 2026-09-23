use bytemuck::{Pod, Zeroable};
use std::{
    ffi::{CStr, CString, c_char, c_float, c_int, c_ushort},
    path::Path,
    str::FromStr,
    thread,
    time::Duration,
};

use crate::{animations, render::Live2DDrawCall};

#[link(name = "interaction", kind = "static")]
unsafe extern "C" {
    fn init(project_dir: *const c_char);
    fn getDrawablesCount() -> c_int;
    fn getDrawableTexIndices(index: c_int) -> c_int;
    fn getDrawableRenderingOrder(index: c_int) -> c_int;
    fn getDrawableGeometry(
        index: c_int,
        vertex_count: *mut c_int,
        positions: *mut *const c_float,
        uvs: *mut *const c_float,
        index_count: *mut c_int,
        indices: *mut *const c_ushort,
    );
    fn getDrawableBlendingState(index: c_int) -> c_int;
    pub fn updateModel();
    pub fn getParameterCount() -> c_int;
    pub fn getParameterIds() -> *const *const c_char;
    pub fn getParameterValue(id: c_int) -> c_float;
    pub fn setParameterValue(id: c_int, value: c_float);
    fn getPartCount() -> c_int;
    fn getPartIds() -> *const *const c_char;
    fn getPartOpacity(id: c_int) -> c_float;
    fn getDrawableOpacity(index: c_int) -> c_float;
    pub fn getParameterId(name: *const c_char) -> c_int;
    pub fn getMasksCount(idx: c_int) -> c_int;
    pub fn getMasks(idx: c_int) -> *const c_int;
    pub fn isDrawableVisible(idx: c_int) -> c_int;
    pub fn getDrawableParentPartIndex(idx: c_int) -> c_int;
    pub fn setPartOpacity(idx: c_int, val: c_float);
}

pub fn c_char_ptr(val: &str) -> *const i8 {
    let c_str = CString::new(val).unwrap();
    let ptr: *const i8 = c_str.as_ptr();

    return ptr;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Live2DVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub opacity: f32,
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn get_parameter_id(name: &str) -> i32 {
    let raw = CString::new(name).unwrap();
    let ptr = raw.as_ptr();

    return getParameterId(ptr);
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn process_live2d_frame() -> Vec<Live2DDrawCall> {
    let count = getDrawablesCount();

    //println!("Drawable count: {}", count);

    if count <= 0 || count > 5000 {
        return Vec::new();
    }

    let mut draw_calls = Vec::with_capacity(count as usize);

    for i in 0..count {
        let is_visible = isDrawableVisible(i);
        if is_visible == 0 {
            continue;
        }

        let texture_idx = getDrawableTexIndices(i);
        let blending = getDrawableBlendingState(i);
        let render_order = getDrawableRenderingOrder(i);
        let mut opacity = getDrawableOpacity(i);

        let mut v_count: c_int = 0;
        let mut i_count: c_int = 0;
        let mut pos_ptr: *const c_float = std::ptr::null();
        let mut uv_ptr: *const c_float = std::ptr::null();
        let mut idx_ptr: *const c_ushort = std::ptr::null();

        getDrawableGeometry(
            i,
            &mut v_count,
            &mut pos_ptr,
            &mut uv_ptr,
            &mut i_count,
            &mut idx_ptr,
        );

        if pos_ptr.is_null() || uv_ptr.is_null() || idx_ptr.is_null() {
            eprintln!("Warning: Null pointer for drawable {}", i);
            continue;
        }

        if v_count <= 0 || i_count <= 0 {
            continue;
        }

        // -- MASKS --
        let mask_count = getMasksCount(i) as usize;
        let mut masks = vec![];

        if mask_count > 0 {
            let mask_ptr = getMasks(i);
            let mask_slice = std::slice::from_raw_parts(mask_ptr, mask_count);
            masks = mask_slice.to_vec();
        }

        // -- ETC --

        let positions = std::slice::from_raw_parts(pos_ptr, (v_count * 2) as usize);
        let uvs = std::slice::from_raw_parts(uv_ptr, (v_count * 2) as usize);
        let indices = std::slice::from_raw_parts(idx_ptr, i_count as usize);

        let positions_vec = positions.to_vec();
        let uvs_vec = uvs.to_vec();
        let mut indices_vec = indices.to_vec();

        if indices_vec.len() % 2 != 0 {
            if let Some(&last) = indices_vec.last() {
                indices_vec.push(last);
            }
        }

        // -- EYES --

        let eye_l_val = getParameterValue(get_parameter_id("ParamEyeLOpen"));
        let eye_r_val = getParameterValue(get_parameter_id("ParamEyeROpen"));

        if mask_count > 0 {
            if eye_l_val < 0.06 && eye_r_val < 0.06 {
                opacity = 0.0;
            }
        }

        if opacity <= 0.01 {
            continue;
        }

        // -- ETC --

        let mut vertices = Vec::with_capacity(v_count as usize);
        for idx in 0..v_count as usize {
            vertices.push(Live2DVertex {
                position: [positions_vec[idx * 2], positions_vec[idx * 2 + 1]],
                uv: [uvs_vec[idx * 2], 1.0 - uvs_vec[idx * 2 + 1]],
                opacity: opacity,
            });
        }

        draw_calls.push(Live2DDrawCall {
            vertices,
            indices: indices_vec,
            texture_idx,
            blend_mode: blending,
            render_order,
            opacity: opacity,
            masks: masks,
            source_index: i as i32,
        });
    }

    //println!("Total draw calls created: {}", draw_calls.len());

    draw_calls.sort_by_key(|dc| dc.render_order);
    draw_calls
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn get_pm_ids() -> Vec<String> {
    let ids_count = getParameterCount();
    let mut res: Vec<String> = vec![];

    println!("{}", ids_count);

    let ids_ptr: *const *const c_char = getParameterIds();

    let ids_slice: &[*const c_char] = std::slice::from_raw_parts(ids_ptr, ids_count as usize);

    for (i, &ptr) in ids_slice.iter().enumerate() {
        if !ptr.is_null() {
            let c_str = CStr::from_ptr(ptr);

            match c_str.to_str() {
                Ok(str_slice) => {
                    res.push(String::from_str(str_slice).unwrap());
                    println!("{} ID: {}", i, str_slice);
                }
                Err(e) => eprintln!("{}", e),
            }
        }
    }

    res
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn get_pr_ids() {
    let count = getPartCount();

    let ids_ptr: *const *const c_char = getPartIds();

    let ids_slice: &[*const c_char] = std::slice::from_raw_parts(ids_ptr, count as usize);

    for (i, &ptr) in ids_slice.iter().enumerate() {
        if !ptr.is_null() {
            let c_str = CStr::from_ptr(ptr);
            let value = getPartOpacity(i as i32);
            match c_str.to_str() {
                Ok(str_slice) => println!("{} ID: {} : {}", i, str_slice, value),
                Err(e) => eprintln!("{}", e),
            }
        }
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn init_model() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");
    let cman = CString::new(manifest_dir).expect("");

    init(cman.as_ptr());

    get_pm_ids();
}
