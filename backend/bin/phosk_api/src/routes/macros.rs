/// `ni!(method, "todo label")` → a stub route handler on that HTTP method,
/// returning `501 Not Implemented` tagged with the todo label.
///
/// Expands at the call site, so everything is fully-qualified: the routing
/// fn via `::axum::routing::$m`, the handler via `$crate::routes::not_impl`.
macro_rules! ni {
    ($m:ident, $label:literal) => {
        ::axum::routing::$m(|| async { $crate::routes::not_impl($label) })
    };
}
