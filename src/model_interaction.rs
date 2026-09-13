use bytemuck::{Pod, Zeroable};
use std::ffi::{CString, c_char, c_float, c_int, c_ushort};

use crate::render::Live2DDrawCall;

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
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Live2DVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn process_live2d_frame() -> Vec<Live2DDrawCall> {
    let count = getDrawablesCount();

    println!("Drawable count: {}", count);

    if count <= 0 || count > 5000 {
        return Vec::new();
    }

    let mut draw_calls = Vec::with_capacity(count as usize);

    for i in 0..count {
        let texture_idx = getDrawableTexIndices(i);
        let blending = getDrawableBlendingState(i);
        let render_order = getDrawableRenderingOrder(i);

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

        let mut vertices = Vec::with_capacity(v_count as usize);
        for idx in 0..v_count as usize {
            vertices.push(Live2DVertex {
                position: [positions_vec[idx * 2], positions_vec[idx * 2 + 1]],
                uv: [uvs_vec[idx * 2], uvs_vec[idx * 2 + 1]],
            });
        }
        if i == 0 && !vertices.is_empty() {
            println!(
                "Raw vertex 0: pos=({:.3}, {:.3}), uv=({:.3}, {:.3})",
                vertices[0].position[0],
                vertices[0].position[1],
                vertices[0].uv[0],
                vertices[0].uv[1]
            );
            println!("Vertex count: {}, Index count: {}", v_count, i_count);
        }

        draw_calls.push(Live2DDrawCall {
            vertices,
            indices: indices_vec,
            texture_idx,
            blend_mode: blending,
            render_order,
        });
    }

    println!("Total draw calls created: {}", draw_calls.len());

    draw_calls.sort_by_key(|dc| dc.render_order);

    draw_calls
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn init_model() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");
    let cman = CString::new(manifest_dir).expect("");

    init(cman.as_ptr());
}
