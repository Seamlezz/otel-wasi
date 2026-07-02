#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_hir;
extern crate rustc_middle;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::ExprKind;
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::TyKind;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects calls to `with_slug` or `error_with_slug` on types that already
    /// implement `WasiError`, which discards the original slug.
    ///
    /// ### Why is this bad?
    /// Wrapping an already-slugged error in another `Error` loses the original
    /// error classification. The inner slug is silently dropped.
    ///
    /// ### Example
    /// ```rust,ignore
    /// let e = wasi_error!("db-timeout", "connection lost");
    /// let e2 = e.with_slug("http-timeout"); // BAD: original slug lost
    /// ```
    pub SLUG_ON_WASI_ERROR,
    Deny,
    "calling with_slug or error_with_slug on a type that already implements WasiError"
}

impl<'tcx> LateLintPass<'tcx> for SlugOnWasiError {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx rustc_hir::Expr<'tcx>) {
        let ExprKind::MethodCall(method_name, receiver, ..) = &expr.kind else {
            return;
        };

        let method_str = method_name.ident.as_str();
        if method_str != "with_slug" && method_str != "error_with_slug" {
            return;
        }

        let receiver_ty = cx.typeck_results().expr_ty(receiver);

        let ty_to_check = if method_str == "error_with_slug" {
            extract_result_error_type(receiver_ty)
        } else {
            Some(receiver_ty)
        };

        if let Some(ty) = ty_to_check {
            if let Some(trait_def_id) = find_wasi_error_trait(cx) {
                if clippy_utils::ty::implements_trait(cx, ty, trait_def_id, &[]) {
                    span_lint_and_help(
                        cx,
                        SLUG_ON_WASI_ERROR,
                        expr.span,
                        format!(
                            "calling `{method_str}` on a type that already implements `WasiError`"
                        ),
                        None,
                        "this discards the original slug; use the error directly or remove this call",
                    );
                }
            }
        }
    }
}

fn extract_result_error_type<'tcx>(
    ty: rustc_middle::ty::Ty<'tcx>,
) -> Option<rustc_middle::ty::Ty<'tcx>> {
    if let TyKind::Adt(_def, substs) = ty.kind() {
        if let [_, generic_arg] = substs.as_slice() {
            if let rustc_middle::ty::GenericArgKind::Type(error_ty) = generic_arg.kind() {
                return Some(error_ty);
            }
        }
    }
    None
}

fn find_wasi_error_trait(cx: &LateContext<'_>) -> Option<rustc_hir::def_id::DefId> {
    let tcx = cx.tcx;

    // Strategy A: Find __WasiErrorMarker, navigate to sibling trait WasiError
    if let Some(def_id) = find_trait_via_marker(tcx) {
        return Some(def_id);
    }

    // Strategy B: Match crate name "otel_wasi", find trait in root module
    find_trait_via_crate_name(tcx)
}

fn find_trait_via_marker(tcx: rustc_middle::ty::TyCtxt<'_>) -> Option<rustc_hir::def_id::DefId> {
    for &crate_num in tcx.crates(()) {
        let crate_def_id = crate_num.as_def_id();
        for child in tcx.module_children(crate_def_id) {
            if child.ident.as_str() == "__WasiErrorMarker" {
                let parent = tcx.parent(child.res.opt_def_id()?);
                for sibling in tcx.module_children(parent) {
                    if sibling.ident.as_str() == "WasiError" {
                        return sibling.res.opt_def_id();
                    }
                }
            }
        }
    }
    None
}

fn find_trait_via_crate_name(tcx: rustc_middle::ty::TyCtxt<'_>) -> Option<rustc_hir::def_id::DefId> {
    for &crate_num in tcx.crates(()) {
        if tcx.crate_name(crate_num).as_str() != "otel_wasi" {
            continue;
        }
        let crate_def_id = crate_num.as_def_id();
        for child in tcx.module_children(crate_def_id) {
            if child.ident.as_str() == "WasiError" {
                return child.res.opt_def_id();
            }
        }
    }
    None
}

#[test]
fn ui_pass() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "tests/ui/pass");
}

#[test]
fn ui_fail() {
    dylint_testing::ui_test_examples(env!("CARGO_PKG_NAME"));
}
