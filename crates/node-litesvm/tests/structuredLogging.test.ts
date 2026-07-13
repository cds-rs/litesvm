import { cpiTree } from "litesvm";
import assert from "node:assert/strict";
import { test } from "node:test";

// Structured logs are what `sol_log_data` emits: a "Program data:" line
// whose payload is one or more base64 fields joined by single spaces.
// The CPI tree keeps them apart from plain `msg!` output via the tagged
// union on `frame.logs`, so a renderer can treat text and event payloads
// differently. None of the bundled test programs call `sol_log_data`,
// so these examples drive the parser with fixture logs; the lines are
// exactly what the runtime would produce.
const PROGRAM = "De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44";

test("anchor emit! events surface as data frame logs", () => {
	// Anchor's `emit!` logs a single field: an 8-byte event discriminator
	// followed by the borsh-encoded event.
	const discriminator = Buffer.from([228, 69, 165, 46, 81, 203, 154, 29]);
	const event = Buffer.concat([discriminator, Buffer.from("hello")]);

	const tree = cpiTree([
		`Program ${PROGRAM} invoke [1]`,
		"Program log: Instruction: Subscribe",
		`Program data: ${event.toString("base64")}`,
		"Program log: plain message",
		`Program ${PROGRAM} consumed 6540 of 200000 compute units`,
		`Program ${PROGRAM} success`,
	]);

	const root = tree[0];
	// The "Instruction: X" line is hoisted onto the frame, not kept as a
	// log entry; everything else stays, with its prefix stripped.
	assert.strictEqual(root.instructionName, "Subscribe");
	assert.deepStrictEqual(root.logs, [
		{ type: "data", value: event.toString("base64") },
		{ type: "msg", value: "plain message" },
	]);

	const dataLog = root.logs[0];
	assert.ok(dataLog.type === "data");
	const payload = Buffer.from(dataLog.value, "base64");
	assert.deepStrictEqual(payload.subarray(0, 8), discriminator);
	assert.strictEqual(payload.subarray(8).toString(), "hello");
});

test("multi-field sol_log_data stays space-separated", () => {
	// `sol_log_data(&[b"Hello", b"world"])` logs each field base64-encoded,
	// joined by single spaces; consumers split before decoding.
	const fields = [Buffer.from("Hello"), Buffer.from("world")];
	const line = fields.map((f) => f.toString("base64")).join(" ");

	const tree = cpiTree([
		`Program ${PROGRAM} invoke [1]`,
		`Program data: ${line}`,
		`Program ${PROGRAM} success`,
	]);

	const dataLog = tree[0].logs[0];
	assert.ok(dataLog.type === "data");
	const decoded = dataLog.value
		.split(" ")
		.map((f) => Buffer.from(f, "base64").toString());
	assert.deepStrictEqual(decoded, ["Hello", "world"]);
});
