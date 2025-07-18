use config::Config;

fn main() {
    let settings = Config::builder()
        .add_source(config::File::with_name("../Settings"))
        .add_source(config::Environment::with_prefix("TOPO"))
        .build()
        .unwrap();

    println!(
        "cargo::rustc-env=TOPO_backend_url={}",
        settings.get_string("backend_url").unwrap()
    );
}
