let workspace = Workspace::open("my-project")?;
let pack = build_for_declared_targets("review-pack".as_ref())?;
let source = PackSource::Artifact(PackArtifactSource { registry: None });

let plan = plan_install(&workspace, &pack.artifact, &source, &["claude".into()])?;
for file in &plan.files {
    println!("{:?} {}", file.change, file.path); // nothing is written yet
}
apply(&workspace, &Baselines::none(), plan)?;
