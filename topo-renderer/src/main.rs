use topo_renderer::start;

use color_eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    start();

    Ok(())
}
