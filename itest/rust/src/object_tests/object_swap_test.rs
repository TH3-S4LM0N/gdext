/*
 * Copyright (c) godot-rust; Bromeon and contributors.
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

// Tests that revolve particularly around https://github.com/godot-rust/gdext/issues/23.

// A lot these tests also exist in the `object_test` module, where they test object lifetime rather than type swapping.
// TODO consolidate them, so that it's less likely to forget edge cases.

// Disabled in Release mode, since we don't perform the subtype check there.
#![cfg(debug_assertions)]

use godot::bind::{godot_api, GodotClass};
use godot::builtin::GString;
use godot::engine::{Node, Node3D, Object};
use godot::obj::{Gd, UserClass};

use crate::framework::{expect_panic, itest, TestContext};
use crate::object_tests::object_test::ObjPayload;

/// Swaps `lhs` and `rhs`, then frees both.
///
/// Needed because freeing a `Gd<T>` with wrong runtime type panics, and otherwise we get a memory leak.
///
/// This is a macro because a function needs excessive bounds, e.g.
/// `T: GodotClass<Mem = Mt>, Mt: godot::obj::mem::Memory + godot::obj::mem::PossiblyManual` and then even more for `DerefMut`...
/// Maybe something to improve in the future, as generic programming is quite hard like this...
macro_rules! swapped_free {
    ($lhs:ident, $rhs:ident) => {{
        let mut lhs = $lhs;
        let mut rhs = $rhs;
        std::mem::swap(&mut *lhs, &mut *rhs);

        lhs.free();
        rhs.free();
    }};
}

// ----------------------------------------------------------------------------------------------------------------------------------------------

#[itest]
fn object_subtype_swap_method() {
    let mut node: Gd<Node> = Node::new_alloc();
    let mut node_3d: Gd<Node3D> = Node3D::new_alloc();

    let n_id = node.instance_id();
    let n3_id = node_3d.instance_id();

    std::mem::swap(&mut *node, &mut *node_3d);

    assert_eq!(node.instance_id(), n3_id);
    assert_eq!(node_3d.instance_id(), n_id);

    // Explicitly allowed to call get_class() because it's on Object and every class inherits that.
    assert_eq!(node.get_class(), GString::from("Node3D"));
    assert_eq!(node_3d.get_class(), GString::from("Node"));

    expect_panic("method call on Gd<T> with invalid runtime type", || {
        node_3d.get_position(); // only Node3D has this method
    });

    swapped_free!(node, node_3d);
}

#[itest]
fn object_subtype_swap_clone() {
    let mut obj: Gd<Object> = Object::new_alloc();
    let mut node: Gd<Node> = Node::new_alloc();

    std::mem::swap(&mut *obj, &mut *node);

    expect_panic("clone badly typed Gd<T>", || {
        let _ = node.clone();
    });

    swapped_free!(obj, node);
}

#[itest]
fn object_subtype_swap_free() {
    let mut obj: Gd<Object> = Object::new_alloc();
    let mut node: Gd<Node> = Node::new_alloc();

    let obj_copy = obj.clone();
    let node_copy = node.clone();

    std::mem::swap(&mut *obj, &mut *node);

    expect_panic("free badly typed Gd<T>", || {
        node.free();
    });
    // Do not check obj, because Gd<Object>::free() always works.

    // Free with original type.
    obj_copy.free();
    node_copy.free();
}

#[itest]
fn object_subtype_swap_argument_passing(ctx: &TestContext) {
    let mut obj: Gd<Object> = Object::new_alloc();
    let mut node: Gd<Node> = Node::new_alloc();
    let node2 = obj.clone();

    std::mem::swap(&mut *obj, &mut *node);

    let mut tree = ctx.scene_tree.clone();
    expect_panic("pass badly typed Gd<T> to Godot engine API", || {
        tree.add_child(node);
    });

    swapped_free!(obj, node2);
}

#[itest]
fn object_subtype_swap_bind() {
    let mut obj: Gd<Object> = Object::new_alloc();
    let mut user: Gd<ObjPayload> = ObjPayload::alloc_gd();

    let obj_id = obj.instance_id();
    let user_id = user.instance_id();

    std::mem::swap(&mut *obj, &mut *user);

    assert_eq!(obj.instance_id(), user_id);
    assert_eq!(user.instance_id(), obj_id);
    assert_eq!(obj.get_class(), GString::from("ObjPayload"));
    assert_eq!(user.get_class(), GString::from("Object"));

    expect_panic("access badly typed Gd<T> using bind()", || {
        let _ = user.bind();
    });
    expect_panic("access badly typed Gd<T> using bind_mut()", || {
        let _ = user.bind_mut();
    });

    swapped_free!(obj, user);
}

#[itest]
fn object_subtype_swap_casts() {
    let mut obj: Gd<Object> = Object::new_alloc();
    let mut node3d: Gd<Node3D> = Node3D::new_alloc();
    let mut obj_v2: Gd<Object> = obj.clone();
    let mut node3d_v2: Gd<Node3D> = node3d.clone();
    let mut obj_v3: Gd<Object> = obj.clone();
    let mut node3d_v3: Gd<Node3D> = node3d.clone();

    // let obj_id = obj.instance_id();
    let node3d_id = node3d.instance_id();

    std::mem::swap(&mut *obj, &mut *node3d);
    std::mem::swap(&mut *obj_v2, &mut *node3d_v2);
    std::mem::swap(&mut *obj_v3, &mut *node3d_v3);
    drop(node3d_v3); // not needed, just existed as a swap partner for obj_v3.

    // Current design: ALL casts fail if self is badly typed, even with correct target type. See RawGd::ffi_cast() for details.

    // Upcasting itself should not fail as long as the target type is matching the runtime type.
    expect_panic("upcast() on Gd<T> with invalid runtime type", || {
        let _upcast_obj = node3d_v2.upcast::<Object>();
        // assert_eq!(upcast_obj.instance_id(), obj_id);
    });

    // Upcasting to itself works, as long as self's type info is correct _before_ the cast.
    let upcast_node3d = obj_v2.upcast::<Object>();
    assert_eq!(upcast_node3d.instance_id(), node3d_id);

    // Downcasting should work if the actual dynamic type matches.
    let downcast_node = obj_v3.cast::<Node3D>();
    assert_eq!(downcast_node.instance_id(), node3d_id);

    // Downcasting does not work if the dynamic type is wrong.
    expect_panic("cast() on Gd<T> with invalid runtime type", || {
        let _ = node3d.clone().cast::<Node3D>();
    });

    swapped_free!(obj, node3d);
}

#[itest(focus)]
fn object_subtype_swap_func_return() {
    let mut swapped = SwapHolder::new_gd();

    // Call through Godot.
    let result = swapped.call("return_swapped_node".into(), &[]);
    dbg!(result);
}

//----------------------------------------------------------------------------------------------------------------------------------------------

#[derive(GodotClass)]
#[class(init)]
struct SwapHolder {
    gc: Vec<Gd<Object>>,
}

#[godot_api]
impl SwapHolder {
    #[func]
    fn return_swapped_node(&mut self) -> Gd<Node> {
        let mut object: Gd<Object> = Object::new_alloc();
        let mut node: Gd<Node> = Node::new_alloc();
        self.gc.push(object.clone());
        self.gc.push(node.clone().upcast());

        std::mem::swap(&mut *object, &mut *node);

        // Dynamic free which is unchecked
        object.call("free".into(), &[]);

        node
    }
}

impl Drop for SwapHolder {
    fn drop(&mut self) {
        for obj in self.gc.drain(..) {
            println!("sw free");
            obj.free();
            println!("after free");
        }
    }
}
