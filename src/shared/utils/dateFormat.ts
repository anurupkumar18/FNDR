export function formatHomeDate(now: Date): string {
    const weekday = now.toLocaleDateString("en-US", { weekday: "long" }).toUpperCase();
    const month   = now.toLocaleDateString("en-US", { month: "long" }).toUpperCase();
    const day     = now.getDate();
    return `${weekday} • ${month} ${day}`;
}
