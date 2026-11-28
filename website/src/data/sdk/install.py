import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.build("review-pack")

plan = workspace.plan_install(pack, ["claude"])
for file in plan.files:
    print(file.change, file.path)  # nothing is written yet

workspace.apply(plan)
