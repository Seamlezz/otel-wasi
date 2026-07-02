use std::fmt::Display;

trait WithSlug: Display + Sized {
    fn with_slug(self, _slug: &'static str) -> String {
        self.to_string()
    }
}

impl<T: Display + Sized> WithSlug for T {}

trait ResultWithSlug<T, E: Display> {
    fn error_with_slug(self, _slug: &'static str) -> Result<T, String>;
}

impl<T, E: Display> ResultWithSlug<T, E> for Result<T, E> {
    fn error_with_slug(self, slug: &'static str) -> Result<T, String> {
        self.map_err(|e| e.with_slug(slug))
    }
}

fn external_call() -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Other, "something went wrong"))
}

fn main() {
    // with_slug on std::io::Error — should NOT trigger the lint
    let _ = external_call().map_err(|e| e.with_slug("external-call-failed"));

    // error_with_slug on Result<T, std::io::Error> — should NOT trigger the lint
    let _ = external_call().error_with_slug("external-call-failed");
}
