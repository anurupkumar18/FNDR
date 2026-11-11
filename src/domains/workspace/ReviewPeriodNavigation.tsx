import { SegmentedControl } from "@/shared/components/SegmentedControl";

export type ReviewPeriod = "day" | "week" | "month";

interface ReviewPeriodNavigationProps {
    value: ReviewPeriod;
    onChange: (period: ReviewPeriod) => void;
}

export function ReviewPeriodNavigation({ value, onChange }: ReviewPeriodNavigationProps) {
    return (
        <SegmentedControl
            ariaLabel="Review period"
            value={value}
            onChange={onChange}
            options={[
                { value: "day", label: "Day" },
                { value: "week", label: "Week" },
                { value: "month", label: "Month" },
            ]}
        />
    );
}
