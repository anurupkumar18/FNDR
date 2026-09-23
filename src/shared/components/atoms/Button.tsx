import type { ButtonHTMLAttributes, ReactNode } from "react";
import { MetalFx } from "metal-fx";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "alarm";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
    variant?: ButtonVariant;
    /** Use mono caps styling (typewriter look). */
    mono?: boolean;
    icon?: ReactNode;
    children?: ReactNode;
}

export function Button({
    variant = "secondary",
    mono = false,
    icon,
    children,
    className,
    type = "button",
    ...rest
}: ButtonProps) {
    const cls = [
        "fndr-button",
        `fndr-button--${variant}`,
        mono ? "fndr-button--mono" : "",
        className ?? "",
    ]
        .filter(Boolean)
        .join(" ");

    const button = (
        <button type={type} className={cls} {...rest}>
            {icon}
            {children}
        </button>
    );

    // Each wrapped button holds its own WebGL context, so the shader is spent on
    // the call to action and nothing else. Without WebGL2 it renders the child.
    return variant === "primary" ? <MetalFx preset="chromatic">{button}</MetalFx> : button;
}

export default Button;
