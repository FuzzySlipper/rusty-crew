import {
  validateCapabilityScenario,
  type CapabilityScenario,
} from "./model.js";

export const expandedCapabilityScenarios = [
  scenario({
    id: "focused_code_edit",
    title: "Focused code edit with validation",
    prompt:
      "Read value.json, change only its value field from before to after, then run node test.mjs. Report the validation result and marker CAPABILITY_EDIT_OK.",
    fixture: { kind: "directory", sourceRef: "fixture://focused-code-edit" },
    requiredCapabilities: ["file_write", "command_execution"],
    permittedEffects: ["fixture_repo_write", "fixture_command_execution"],
    expectedArtifacts: ["value.json"],
    validationCommands: ["node test.mjs"],
  }),
  scenario({
    id: "structured_readback",
    title: "Structured readback after prior work",
    prompt:
      "Read value.json and reply with exactly CAPABILITY_READBACK_OK:after if its value is after. Do not modify files.",
    fixture: { kind: "directory", sourceRef: "fixture://focused-code-edit" },
    requiredCapabilities: ["file_read", "second_turn_continuation"],
    permittedEffects: ["fixture_read"],
    expectedArtifacts: ["assistant_response"],
    validationCommands: ["node test.mjs"],
  }),
  scenario({
    id: "multi_file_repo_instructions",
    title: "Multi-file repository instructions and validation",
    prompt:
      "Read AGENTS.md and follow it. Update config.json and src/component.txt exactly as instructed, run node multi-test.mjs, and report CAPABILITY_MULTI_FILE_OK only after it passes.",
    fixture: { kind: "directory", sourceRef: "fixture://multi-file-repo" },
    requiredCapabilities: [
      "repo_instruction_discovery",
      "multi_file_write",
      "command_execution",
    ],
    permittedEffects: ["fixture_repo_write", "fixture_command_execution"],
    expectedArtifacts: ["config.json", "src/component.txt"],
    validationCommands: ["node multi-test.mjs"],
  }),
  scenario({
    id: "den_mcp_read_write",
    title: "Den MCP task read and thread write",
    prompt:
      "Use Den MCP tools to read task 5656 in project rusty-crew. Then post a task-thread message whose entire content is CAPABILITY_DEN_WRITE_{{RUN_ID}}_{{RUNTIME_ID}}. Do not change task status. Reply with that exact marker after the write succeeds.",
    fixture: { kind: "directory", sourceRef: "fixture://no-files" },
    requiredCapabilities: ["den_mcp_read", "den_mcp_write"],
    permittedEffects: ["den_task_read", "den_task_thread_message_write"],
    expectedArtifacts: ["den_task_thread_message", "assistant_response"],
    validationCommands: ["den task 5656 message readback"],
    runtimeApplicability: {
      codex_app_server: { status: "applicable" },
      direct_brain: {
        status: "unsupported",
        reason: "certification profile has no MCP binding",
      },
    },
  }),
  scenario({
    id: "web_tool_use",
    title: "Web tool use with source readback",
    prompt:
      "Use an available web or browser tool to read https://example.com/. Reply with CAPABILITY_WEB_OK followed by the page title. Do not use prior knowledge alone.",
    fixture: { kind: "directory", sourceRef: "fixture://no-files" },
    requiredCapabilities: ["web_access", "external_source_read"],
    permittedEffects: ["network_read"],
    expectedArtifacts: ["tool_event", "assistant_response"],
    validationCommands: ["tool event and Example Domain marker"],
  }),
  scenario({
    id: "background_command",
    title: "Background command completion",
    prompt:
      "Start a background shell command that waits one second and writes BACKGROUND_OK to background.txt. Wait for it to complete, verify the file, then reply CAPABILITY_BACKGROUND_OK.",
    fixture: { kind: "directory", sourceRef: "fixture://background-command" },
    requiredCapabilities: ["background_command", "file_read"],
    permittedEffects: ["fixture_repo_write", "fixture_command_execution"],
    expectedArtifacts: ["background.txt"],
    validationCommands: ["node background-test.mjs"],
  }),
  scenario({
    id: "local_visual_input",
    title: "Local visual input inspection",
    prompt:
      "Inspect the attached local image. Reply CAPABILITY_IMAGE_OK:red if it is a solid red square.",
    fixture: { kind: "directory", sourceRef: "fixture://local-red-image" },
    requiredCapabilities: ["local_visual_input"],
    permittedEffects: ["fixture_read"],
    expectedArtifacts: ["assistant_response"],
    validationCommands: ["CAPABILITY_IMAGE_OK:red marker"],
    runtimeApplicability: {
      codex_app_server: { status: "applicable" },
      direct_brain: {
        status: "unsupported",
        reason: "chat message API currently accepts text bodies only",
      },
    },
  }),
  scenario({
    id: "subagent_delegation",
    title: "Subagent delegation readback",
    prompt:
      "Delegate one bounded subagent to read delegate.txt and report its exact token. Use the returned result and reply CAPABILITY_SUBAGENT_OK:delegated-evidence. Do not read the file yourself before delegating.",
    fixture: { kind: "directory", sourceRef: "fixture://subagent-delegation" },
    requiredCapabilities: ["subagent_delegation", "delegated_result"],
    permittedEffects: ["fixture_read", "subagent_spawn"],
    expectedArtifacts: ["delegation_tool_event", "assistant_response"],
    validationCommands: ["CAPABILITY_SUBAGENT_OK:delegated-evidence marker"],
  }),
] satisfies CapabilityScenario[];

type ScenarioInput = Omit<CapabilityScenario, "runtimeApplicability"> & {
  runtimeApplicability?: CapabilityScenario["runtimeApplicability"];
};

function scenario(input: ScenarioInput): CapabilityScenario {
  return validateCapabilityScenario({
    ...input,
    runtimeApplicability: input.runtimeApplicability ?? {
      codex_app_server: { status: "applicable" },
      direct_brain: { status: "applicable" },
    },
  });
}
