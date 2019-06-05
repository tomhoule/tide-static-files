use tide::middleware::RootLogger;
use tide_static_files::StaticFiles;

fn main() {
    let mut app = tide::App::new(());

    app.middleware(RootLogger::new());

    let static_files = StaticFiles::new(".").unwrap();
    app.at("/static/*").get(static_files);

    app.serve("127.0.0.1:8000").unwrap();
}
