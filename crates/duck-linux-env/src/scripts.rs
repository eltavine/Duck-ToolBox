use crate::{model::Config, store};
use anyhow::Result;
use std::{fs, path::Path};
pub fn write(root: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    let run = root.join("run/duck-toolbox");
    store::private_dir(&run)?;
    fs::create_dir(run.join("services"))?;
    fs::create_dir(run.join("status"))?;
    let mut script = String::from("#!/bin/sh\ntrap 'exit 0' TERM INT\n");
    for service in config.services.iter().filter(|s| s.enabled) {
        fs::write(
            run.join(format!("services/{}.command", service.id)),
            &service.command,
        )?;
        let wrapper = format!(
            "#!/bin/sh\n/bin/cat /proc/$$/stat > /run/duck-toolbox/status/{id}.stat\nprintf '[{id}] starting\\n'\n/bin/sh /run/duck-toolbox/services/{id}.command\nresult=$?\nprintf '%s\\n' \"$result\" > /run/duck-toolbox/status/{id}.exit\nprintf '[{id}] exited %s\\n' \"$result\"\n",
            id = service.id
        );
        fs::write(run.join(format!("services/{}.sh", service.id)), wrapper)?;
        script.push_str(&format!(
            "/bin/sh /run/duck-toolbox/services/{}.sh &\n",
            service.id
        ));
    }
    script.push_str("while :; do /bin/sleep 3600 & wait $! || :; done\n");
    fs::write(run.join("init.sh"), script)?;
    Ok(())
}
