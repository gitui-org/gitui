use bugreport::{
	bugreport,
	collector::{
		CommandLine, CompileTimeInformation, EnvironmentVariables,
		OperatingSystem, SoftwareVersion,
	},
	format::Markdown,
};

pub fn generate_bugreport() {
	bugreport!()
		.info(SoftwareVersion::default())
		.info(OperatingSystem::default())
		.info(CompileTimeInformation::default())
		.info(EnvironmentVariables::list(&[
			"SHELL",
			"EDITOR",
			"GIT_EDITOR",
			"VISUAL",
		]))
		.info(CommandLine::default())
		.print::<Markdown>();
}
