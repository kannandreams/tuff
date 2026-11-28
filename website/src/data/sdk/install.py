import tuff_sdk as tuff

# 1. Open your project and read a pack.
workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.build("review-pack")

# 2. Plan the install. Nothing is written yet.
plan = workspace.plan_install(pack, ["claude"])
for file in plan.files:
    print(file.change, file.path)

# 3. Apply the plan once the user approves it.
workspace.apply(plan)
