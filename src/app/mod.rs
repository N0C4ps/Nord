mod dependencies;

use std::error::Error;

pub fn run() -> Result<(), Box<dyn Error>> {
    let dependencies = dependencies::AppDependencies::build()?;
    dependencies.logger.info(dependencies.settings.title);
    dependencies.logger.info(&format!(
        "Platform: {:?} ({})",
        dependencies.platform.backend(),
        dependencies.platform.architecture()
    ));
    crate::gameloop::run();
    Ok(())
}
