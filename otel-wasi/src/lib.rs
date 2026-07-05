use opentelemetry::{
    KeyValue,
    trace::{Status, TraceContextExt, TracerProvider as _},
};
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use opentelemetry_wasi::{TraceContextPropagator, WasiPropagator, WasiSpanProcessor};
use std::{
    cell::RefCell,
    fmt::{self, Display},
    future::Future,
    pin::Pin,
    sync::OnceLock,
    task::{Context, Poll},
    time::Instant,
};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{Registry, layer::SubscriberExt};

pub use opentelemetry::KeyValue as Attribute;
#[cfg(feature = "macros")]
pub use otel_wasi_macros::wasi_instrument;
pub use tracing::span;

const SPAN_DROPPED_SLUG: &str = "otel-wasi-span-dropped-without-finish";

static TRACE_INIT: OnceLock<()> = OnceLock::new();

thread_local! {
    static MAIN_SPAN_STACK: RefCell<Vec<Span>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone)]
pub struct SpanConfig {
    service_name: &'static str,
    span_name: &'static str,
}

impl SpanConfig {
    pub fn builder() -> SpanConfigBuilder {
        SpanConfigBuilder::default()
    }

    pub fn service_name(&self) -> &'static str {
        self.service_name
    }

    pub fn span_name(&self) -> &'static str {
        self.span_name
    }
}

#[derive(Debug, Default, Clone)]
pub struct SpanConfigBuilder {
    service_name: Option<&'static str>,
    span_name: Option<&'static str>,
}

impl SpanConfigBuilder {
    pub fn service_name(mut self, service_name: &'static str) -> Self {
        self.service_name = Some(service_name);
        self
    }

    pub fn span_name(mut self, span_name: &'static str) -> Self {
        self.span_name = Some(span_name);
        self
    }

    pub fn build(self) -> SpanConfig {
        let service_name = self.service_name.expect(
            "SpanConfig requires service_name; use SpanConfig::builder().service_name(...)",
        );
        let span_name = self
            .span_name
            .expect("SpanConfig requires span_name; use SpanConfig::builder().span_name(...)");

        SpanConfig {
            service_name,
            span_name,
        }
    }
}

/// An error that carries its own slug for OTel span recording.
pub trait WasiError {
    /// A stable slug identifying this error class (e.g. "auth-callout-timeout").
    fn slug(&self) -> &'static str;

    /// A human-readable message for the span's `exception.message` attribute.
    fn message(&self) -> String;
}

/// Hidden marker type used by the `otel-wasi-dylint` lint to locate the
/// [`WasiError`] trait at compile time. Not part of the public API.
#[doc(hidden)]
pub struct __WasiErrorMarker;

/// A convenience error type implementing [`WasiError`].
///
/// The generic parameter `E` is the payload that travels with the error. For
/// plain string exports it defaults to `String`; for typed component exports it
/// can be the WIT error type (e.g. `ErrorCode`). The payload is also the span
/// message via its `Display` implementation.
#[derive(Debug, Clone)]
pub struct Error<E = String> {
    slug: &'static str,
    inner: E,
}

impl Error<String> {
    pub fn new(slug: &'static str, message: impl Display) -> Self {
        let message = message.to_string();
        Self {
            slug,
            inner: message,
        }
    }
}

impl<E> Error<E> {
    /// Consume the error and return the original payload.
    pub fn into_inner(self) -> E {
        self.inner
    }

    pub fn map_inner<U>(self, f: impl FnOnce(E) -> U) -> Error<U> {
        Error {
            slug: self.slug,
            inner: f(self.inner),
        }
    }
}

impl<E: Display> Error<E> {
    pub fn into_message(self) -> Error<String> {
        self.map_inner(|inner| inner.to_string())
    }
}

impl<E: Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<E: Display> WasiError for Error<E> {
    fn slug(&self) -> &'static str {
        self.slug
    }

    fn message(&self) -> String {
        self.inner.to_string()
    }
}

/// Creates an [`Error`] with a slug and a [`format!`]-style message.
#[macro_export]
macro_rules! wasi_error {
    ($slug:literal, $($arg:tt)*) => {
        $crate::Error::new($slug, format!($($arg)*))
    };
}

/// Extension trait to attach a slug to any type.
pub trait WithSlug: Sized {
    fn with_slug(self, slug: &'static str) -> Error<String>
    where
        Self: Display;

    fn with_typed_slug(self, slug: &'static str) -> Error<Self>;
}

impl<T: Sized> WithSlug for T {
    fn with_slug(self, slug: &'static str) -> Error<String>
    where
        T: Display,
    {
        Error::new(slug, self)
    }

    fn with_typed_slug(self, slug: &'static str) -> Error<Self> {
        Error { slug, inner: self }
    }
}

/// Extension trait to attach a slug to the error of a [`Result`]
pub trait ResultWithSlug<T, E: Sized> {
    fn error_with_slug(self, slug: &'static str) -> Result<T, Error<String>>
    where
        E: Display;

    fn error_with_typed_slug(self, slug: &'static str) -> Result<T, Error<E>>;
}

impl<T, E: Sized> ResultWithSlug<T, E> for Result<T, E> {
    fn error_with_slug(self, slug: &'static str) -> Result<T, Error<String>>
    where
        E: Display,
    {
        self.map_err(|e| Error::new(slug, e))
    }

    fn error_with_typed_slug(self, slug: &'static str) -> Result<T, Error<E>> {
        self.map_err(|e| Error { slug, inner: e })
    }
}

pub trait SpanOutcome {
    fn record_on(&self, span: &WasiSpan);
}

impl SpanOutcome for () {
    fn record_on(&self, span: &WasiSpan) {
        span.set_status_ok();
    }
}

impl<T, E: WasiError> SpanOutcome for Result<T, E> {
    fn record_on(&self, span: &WasiSpan) {
        match self {
            Ok(_) => span.set_status_ok(),
            Err(e) => span.set_status_error(e.slug(), e.message()),
        }
    }
}

pub struct WasiSpan {
    span: Span,
    started_at: Instant,
    finished: bool,
}

impl WasiSpan {
    /// Create a WasiSpan from an already-constructed tracing Span.
    ///
    /// The tracing span's name becomes the OTel span name directly.
    /// Prefer this over [`start`] when the span name is available as
    /// a string literal (e.g. from the `#[wasi_instrument]` macro).
    pub fn from_span(span: Span, config: SpanConfig) -> Self {
        ensure_init(config.service_name);

        let parent_cx = TraceContextPropagator::new().extract(&opentelemetry::Context::current());
        let _ = span.set_parent(parent_cx);

        span.set_attribute(
            "trace_id",
            span.context().span().span_context().trace_id().to_string(),
        );
        span.set_attribute("service.name", config.service_name);
        span.set_attribute("service.version", env!("CARGO_PKG_VERSION"));

        if let Some(git_hash) = option_env!("GIT_HASH") {
            span.set_attribute("service.build.git_hash", git_hash);
        }

        WasiSpan {
            span,
            started_at: Instant::now(),
            finished: false,
        }
    }

    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn set_attribute(&self, attr: KeyValue) {
        self.span.set_attribute(attr.key, attr.value);
    }

    pub fn set_attributes(&self, attrs: impl IntoIterator<Item = KeyValue>) {
        for attr in attrs {
            self.set_attribute(attr);
        }
    }

    pub fn finish<R: SpanOutcome>(mut self, result: &R) {
        self.finished = true;
        self.record_duration();
        result.record_on(&self);
    }

    pub fn finish_ok(mut self) {
        self.finished = true;
        self.record_duration();
        self.set_status_ok();
    }

    pub fn finish_error(mut self, slug: &'static str, message: impl Display) {
        self.finished = true;
        self.record_duration();
        self.set_status_error(slug, message);
    }

    fn record_duration(&self) {
        self.span
            .set_attribute("duration_ms", self.started_at.elapsed().as_millis() as i64);
    }

    fn set_status_ok(&self) {
        self.span.set_status(Status::Ok);
    }

    fn set_status_error(&self, slug: &'static str, message: impl Display) {
        let message = message.to_string();
        self.span.set_attribute("error", true);
        self.span.set_attribute("exception.slug", slug);
        self.span
            .set_attribute("exception.message", message.clone());
        self.span.set_status(Status::error(message));
    }
}

impl Drop for WasiSpan {
    fn drop(&mut self) {
        if self.finished {
            return;
        }

        self.record_duration();
        self.set_status_error(
            SPAN_DROPPED_SLUG,
            "WasiSpan dropped before finish was called",
        );
    }
}

pub struct MainSpanGuard {
    _private: (),
}

pub fn enter_main_span(span: Span) -> MainSpanGuard {
    MAIN_SPAN_STACK.with(|stack| stack.borrow_mut().push(span));
    MainSpanGuard { _private: () }
}

impl Drop for MainSpanGuard {
    fn drop(&mut self) {
        MAIN_SPAN_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub fn current_main_span() -> Option<Span> {
    MAIN_SPAN_STACK.with(|stack| stack.borrow().last().cloned())
}

pub fn set_current_attributes(attrs: impl IntoIterator<Item = KeyValue>) {
    let span = Span::current();
    set_span_attributes(&span, attrs);
}

pub fn set_main_attributes(attrs: impl IntoIterator<Item = KeyValue>) {
    let Some(span) = current_main_span() else {
        debug_assert!(
            false,
            "otel_wasi::main_attribute! called outside #[wasi_instrument] main span"
        );
        return;
    };

    set_span_attributes(&span, attrs);
}

pub struct WithMainSpan<F> {
    main_span: Span,
    future: Pin<Box<F>>,
}

pub fn with_main_span<F>(main_span: Span, future: F) -> WithMainSpan<F>
where
    F: Future,
{
    WithMainSpan {
        main_span,
        future: Box::pin(future),
    }
}

impl<F> Future for WithMainSpan<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let guard = enter_main_span(self.main_span.clone());
        let poll = self.future.as_mut().poll(cx);
        drop(guard);
        poll
    }
}

pub fn set_span_attributes(span: &Span, attrs: impl IntoIterator<Item = KeyValue>) {
    for attr in attrs {
        span.set_attribute(attr.key, attr.value);
    }
}

#[macro_export]
macro_rules! attribute {
    (span: $span:expr, $($key:literal = $value:expr),+ $(,)?) => {{
        $crate::set_span_attributes(
            $span,
            [$($crate::Attribute::new($key, $value)),+],
        );
    }};

    ($($key:literal = $value:expr),+ $(,)?) => {{
        $crate::set_current_attributes([
            $($crate::Attribute::new($key, $value)),+
        ]);
    }};
}

#[macro_export]
macro_rules! main_attribute {
    ($($key:literal = $value:expr),+ $(,)?) => {{
        $crate::set_main_attributes([
            $($crate::Attribute::new($key, $value)),+
        ]);
    }};
}

fn ensure_init(service_name: &'static str) {
    TRACE_INIT.get_or_init(|| {
        let provider = SdkTracerProvider::builder()
            .with_span_processor(WasiSpanProcessor::new())
            .with_resource(Resource::builder().with_service_name(service_name).build())
            .build();
        let tracer = provider.tracer(service_name);
        let subscriber =
            Registry::default().with(tracing_opentelemetry::layer().with_tracer(tracer));

        let _ = tracing::subscriber::set_global_default(subscriber);
        opentelemetry::global::set_tracer_provider(provider);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_span_context_is_scoped() {
        assert!(current_main_span().is_none());

        let first = Span::none();
        let first_guard = enter_main_span(first);
        assert!(current_main_span().is_some());

        {
            let second = Span::none();
            let _second_guard = enter_main_span(second);
            assert!(current_main_span().is_some());
        }

        assert!(current_main_span().is_some());
        drop(first_guard);
        assert!(current_main_span().is_none());
    }

    #[test]
    fn set_main_attributes_uses_registered_span() {
        let span = Span::none();
        let _guard = enter_main_span(span);

        set_main_attributes([KeyValue::new("test.value", 1_i64)]);
    }
}
