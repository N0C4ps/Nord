mod dependencies;

pub fn run() -> Result<(), nord_dependency::DependencyError> {
    let dependencies = dependencies::AppDependencies::build()?;
    dependencies.logger.info(dependencies.settings.title);
    crate::gameloop::run();
    Ok(())
}
