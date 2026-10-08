import type { ReactNode } from "react";
import type { AttachedMemory } from "@/shared/ipc/tauri";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";

interface AgentReplyProps {
    content: string;
    /** The memories sent with the message this answers, in the order they were numbered. */
    cited: AttachedMemory[];
    onOpenMemory: (memory: AttachedMemory) => void;
}

const INLINE = /(`[^`\n]+`|\*\*[^*\n]+\*\*|\[[^\]\n]+\]\(https?:\/\/[^\s)]+\)|\[\d{1,2}\])/g;
const BULLET = /^\s*[-*•]\s+/;
const NUMBERED = /^\s*\d+[.)]\s+/;
const HEADING = /^#{1,4}\s+/;

type Block =
    | { kind: "paragraph"; lines: string[] }
    | { kind: "heading"; text: string }
    | { kind: "list"; ordered: boolean; items: string[] }
    | { kind: "code"; text: string };

/** Splits an answer into the few shapes a model's plain-text reply uses.
 *  Anything it does not recognize stays as ordinary text. */
export function replyBlocks(content: string): Block[] {
    const blocks: Block[] = [];
    let code: string[] | null = null;
    for (const line of content.split("\n")) {
        if (line.trim().startsWith("```")) {
            if (code) {
                blocks.push({ kind: "code", text: code.join("\n") });
                code = null;
            } else {
                code = [];
            }
            continue;
        }
        if (code) {
            code.push(line);
            continue;
        }
        const last = blocks[blocks.length - 1];
        if (!line.trim()) {
            blocks.push({ kind: "paragraph", lines: [] });
        } else if (HEADING.test(line)) {
            blocks.push({ kind: "heading", text: line.replace(HEADING, "") });
        } else if (BULLET.test(line) || NUMBERED.test(line)) {
            const ordered = NUMBERED.test(line);
            const item = line.replace(ordered ? NUMBERED : BULLET, "");
            if (last?.kind === "list" && last.ordered === ordered) last.items.push(item);
            else blocks.push({ kind: "list", ordered, items: [item] });
        } else if (last?.kind === "paragraph") {
            last.lines.push(line);
        } else {
            blocks.push({ kind: "paragraph", lines: [line] });
        }
    }
    // An answer cut off inside a code block still shows what arrived.
    if (code) blocks.push({ kind: "code", text: code.join("\n") });
    return blocks.filter((block) => block.kind !== "paragraph" || block.lines.length > 0);
}

/**
 * A Hermes answer: paragraphs, lists, code and links as such, and `[2]` as a
 * button that opens the memory it cites. Built from text nodes only, so
 * nothing in an answer can run as markup.
 */
export function AgentReply({ content, cited, onOpenMemory }: AgentReplyProps) {
    const inline = (text: string, keyPrefix: string): ReactNode[] =>
        text.split(INLINE).map((part, index) => {
            const key = `${keyPrefix}-${index}`;
            if (/^`[^`]+`$/.test(part)) return <code key={key}>{part.slice(1, -1)}</code>;
            if (/^\*\*[^*]+\*\*$/.test(part)) return <strong key={key}>{part.slice(2, -2)}</strong>;
            const link = /^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/.exec(part);
            if (link) {
                return (
                    <a
                        key={key}
                        href={link[2]}
                        onClick={(event) => {
                            event.preventDefault();
                            void openExternalUrl(link[2]);
                        }}
                    >
                        {link[1]}
                    </a>
                );
            }
            const memory = /^\[\d{1,2}\]$/.test(part) ? cited[Number(part.slice(1, -1)) - 1] : undefined;
            if (memory) {
                return (
                    <button
                        key={key}
                        type="button"
                        className="aw-citation"
                        title={memory.title}
                        aria-label={`Open memory ${part.slice(1, -1)}: ${memory.title}`}
                        onClick={() => onOpenMemory(memory)}
                    >
                        {part}
                    </button>
                );
            }
            return part;
        });

    return (
        <div className="aw-bubble aw-reply">
            {replyBlocks(content).map((block, index) => {
                const key = `b${index}`;
                switch (block.kind) {
                    case "code":
                        return (
                            <pre key={key}>
                                <code>{block.text}</code>
                            </pre>
                        );
                    case "heading":
                        return (
                            <p key={key}>
                                <strong>{inline(block.text, key)}</strong>
                            </p>
                        );
                    case "list": {
                        const items = block.items.map((item, i) => <li key={`${key}-${i}`}>{inline(item, `${key}-${i}`)}</li>);
                        return block.ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>;
                    }
                    case "paragraph":
                        return (
                            <p key={key}>
                                {block.lines.map((line, i) => (
                                    <span key={`${key}-${i}`}>
                                        {i > 0 ? <br /> : null}
                                        {inline(line, `${key}-${i}`)}
                                    </span>
                                ))}
                            </p>
                        );
                }
            })}
        </div>
    );
}
