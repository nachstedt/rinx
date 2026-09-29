"""
plantuml_tool rule.

Runs a PlantUML `java_binary` on a Java runtime this repository chooses,
rather than on whatever `--tool_java_runtime_version` the consuming build
happens to set (Bazel's default is JDK 11).

`rinx_site` lays diagrams out with `-Playout=elk`, and the ELK classes
bundled in the PlantUML jar are compiled for Java 21: on an older runtime every
diagram needing a layout fails with `UnsupportedClassVersionError`. A
`.bazelrc` line would fix only this repository, never a module depending on
it, so the runtime is pinned here by a transition on the one dependency edge
that runs PlantUML — no other target's configuration changes. See ADR-012 §8.
"""

# The oldest runtime the jar's ELK classes load on (class file version 65).
_PLANTUML_JAVA_RUNTIME = "remotejdk_21"

def _pin_java_runtime_impl(_settings, _attr):
    # Reached from `rinx_site`'s `cfg = "exec"` attribute, so this already is
    # the exec configuration, whose Java runtime is `java_runtime_version`.
    return {"//command_line_option:java_runtime_version": _PLANTUML_JAVA_RUNTIME}

_pin_java_runtime = transition(
    implementation = _pin_java_runtime_impl,
    inputs = [],
    outputs = ["//command_line_option:java_runtime_version"],
)

def _plantuml_tool_impl(ctx):
    binary = ctx.attr.binary[0]
    # The `java_binary` launcher finds its runfiles next to its own path, so
    # the symlink carries the binary's runfiles along with it.
    executable = ctx.actions.declare_file(ctx.label.name)
    ctx.actions.symlink(
        output = executable,
        target_file = binary[DefaultInfo].files_to_run.executable,
        is_executable = True,
    )
    return [DefaultInfo(
        executable = executable,
        runfiles = ctx.runfiles(files = [executable]).merge(binary[DefaultInfo].default_runfiles),
    )]

plantuml_tool = rule(
    implementation = _plantuml_tool_impl,
    doc = "A PlantUML `java_binary`, always run on the Java runtime its jar needs.",
    executable = True,
    attrs = {
        "binary": attr.label(
            mandatory = True,
            executable = True,
            cfg = _pin_java_runtime,
            doc = "The PlantUML `java_binary`.",
        ),
    },
)
