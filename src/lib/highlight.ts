export interface HighlightPart {
    text: string;
    highlighted: boolean;
}

function escapeRegex(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function splitByAnchorTerms(text: string, anchorTerms: string[]): HighlightPart[] {
    const clean = text ?? "";
    if (!clean.trim() || anchorTerms.length === 0) {
        return [{ text: clean, highlighted: false }];
    }

    const terms = [...new Set(anchorTerms.map((term) => term.trim()).filter(Boolean))]
        .sort((left, right) => right.length - left.length)
        .map((term) => escapeRegex(term));

    if (terms.length === 0) {
        return [{ text: clean, highlighted: false }];
    }

    const regex = new RegExp(`(${terms.join("|")})`, "gi");
    const segments = clean.split(regex);

    const highlightMatcher = new RegExp(`^(${terms.join("|")})$`, "i");

    return segments
        .filter((segment) => segment.length > 0)
        .map((segment) => ({
            text: segment,
            highlighted: highlightMatcher.test(segment),
        }));
}
