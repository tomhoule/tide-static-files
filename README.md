# tide-static-files

This is a working prototype intended to gather feedback on static file serving in tide. It uses tokio-fs.

See the docs and tests for more info.

```rust
use tide_static_files::StaticFiles;

fn main() {
    let mut app = tide::App::new(());

    app.middleware(RootLogger::new());

    app.at("/static/*").get(StaticFiles::new(".").unwrap());

    // Do something

    app.serve("127.0.0.1:8000").unwrap();
}
```
