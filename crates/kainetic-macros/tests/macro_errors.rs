//! Compile-fail tests for `#[tool]` and `#[agent]` macro error messages.
//!
//! Each `.rs` file in `tests/ui/` that causes a compile error is matched
//! against a corresponding `.stderr` snapshot. Run `TRYBUILD=overwrite` to
//! regenerate snapshots after changing error messages.

#[test]
fn macro_compile_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/tool_not_async.rs");
    t.compile_fail("tests/ui/tool_wrong_return.rs");
    t.compile_fail("tests/ui/tool_no_params.rs");
    t.compile_fail("tests/ui/tool_unknown_attr.rs");
    t.compile_fail("tests/ui/agent_not_async.rs");
}
