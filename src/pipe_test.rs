use anyhow::Result;
use checklints::command::Pipeline;

fn main() -> Result<()> {
    env_logger::init();
    for (i, cmd) in [
        "echo hello world",
        "echo hello world | tr a-z A-Z",
        "echo hello world | tr a-z A-Z | tr ' ' '\\n'",
    ]
    .iter()
    .enumerate()
    {
        println!("========== Case {i} ==========");
        let pipeline = Pipeline::new(cmd)?;
        let output = pipeline.run(None)?;

        println!("Final stdout:\n{}", output.stdout().unwrap());
    }

    Ok(())
}
