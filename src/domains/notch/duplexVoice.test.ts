import { describe, expect, it } from "vitest";
import { classifyUtterance, isEcho } from "./duplexVoice";

describe("classifyUtterance", () => {
    it("treats short stop phrases as an interrupt at any time", () => {
        expect(classifyUtterance("Stop!", false)).toEqual({ kind: "stop" });
        expect(classifyUtterance("wait wait", true)).toEqual({ kind: "stop" });
        expect(classifyUtterance("never mind", false)).toEqual({ kind: "stop" });
    });

    it("keeps longer instructions that merely contain stop words", () => {
        expect(classifyUtterance("stop at the second tab and open settings", false)).toEqual({
            kind: "say",
            text: "stop at the second tab and open settings",
        });
    });

    it("reads yes and no only while an approval is pending", () => {
        expect(classifyUtterance("okay do it", true)).toEqual({ kind: "approve" });
        expect(classifyUtterance("nope", true)).toEqual({ kind: "decline" });
        expect(classifyUtterance("yes", false)).toEqual({ kind: "say", text: "yes" });
    });

    it("ignores silence", () => {
        expect(classifyUtterance("  ...  ", false)).toBeNull();
    });
});

describe("isEcho", () => {
    it("recognizes FNDR hearing its own words", () => {
        expect(isEcho("okay to click new note", 'Okay to click "New Note" in Notes?')).toBe(true);
    });

    it("lets real interruptions through", () => {
        expect(isEcho("no open the other one", 'Okay to click "New Note" in Notes?')).toBe(false);
        expect(isEcho("anything", "")).toBe(false);
    });
});
