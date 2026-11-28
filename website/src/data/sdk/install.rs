// 1. Open your project and read a pack.
let workspace = Workspace::open("my-project")?;
let pack = build_for_declared_targets("review-pack".as_ref())?;
let source = PackSource::Artifact(PackArtifactSource { registry: None });

// 2. Plan the install. Nothing is written yet.
let plan = plan_install(&workspace, &pack.artifact, &source, &["claude".into()])?;
for file in &plan.files {
    println!("{:?} {}", file.change, file.path);
}

// 3. Apply the plan once the user approves it.
apply(&workspace, &Baselines::none(), plan)?;
