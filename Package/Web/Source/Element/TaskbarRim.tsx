import "../Asset/CSS/Corner.css";

export type TaskbarRimProperty = {
	leftPx: number;
	topPx: number;
	widthPx: number;
	heightPx: number;
	radiusPx: number;
	scale: number;
	dpr: number;
};

/**
 * Hardware-anti-aliased concentric macOS 26 Liquid Glass rim aligned 1:1 in
 * exact physical pixels directly over the clipped Windows taskbar
 * (`Shell_TrayWnd` / `Shell_SecondaryTrayWnd`).
 *
 * Positions the rim using `physicalPx / window.devicePixelRatio` from the exact
 * `(0, 0)` viewport origin (never `%` / `100vh` / `bottom: 0`) so Chromium's
 * integer-DIP viewport rounding at custom/fractional DPIs (e.g. 131 DPI / 136%,
 * 110%, 175%) can never shift the rim downward by 1–2 physical pixels.
 */
export default (Property: TaskbarRimProperty) => {
	const Width = () => Math.max(2, Property.widthPx);
	const Height = () => Math.max(2, Property.heightPx);
	const Dpr = () => Math.max(0.5, Property.dpr || Property.scale || 1);
	const Scale = () => Math.max(1, Property.dpr || Property.scale || 1);
	const Radius = () => Math.min(Property.radiusPx, Math.min(Width(), Height()) / 2);
	const OuterWidth = () => 1.0 * Scale();
	const OuterInset = () => OuterWidth() / 2;
	const OuterRadius = () => Math.max(0, Radius() - OuterInset());
	const InnerWidth = () => 0.75 * Scale();
	const InnerInset = () => OuterWidth() - InnerWidth() / 2 + 0.25 * Scale();
	const InnerRadius = () => Math.max(0, Radius() - InnerInset());

	return (
		<div
			class="TaskbarRim"
			style={`left: ${Property.leftPx / Dpr()}px; top: ${Property.topPx / Dpr()}px; width: ${Width() / Dpr()}px; height: ${Height() / Dpr()}px;`}>
			<svg
				class="TaskbarRim-Svg"
				viewBox={`0 0 ${Width()} ${Height()}`}
				preserveAspectRatio="none"
				aria-hidden="true">
				<defs>
					<linearGradient id="radius-tb-specular" x1="0" y1="0" x2="0.18" y2="1">
						<stop offset="0%" class="TaskbarRim-StopTop" />
						<stop offset="46%" class="TaskbarRim-StopMid" />
						<stop offset="100%" class="TaskbarRim-StopBottom" />
					</linearGradient>
				</defs>
				<rect
					class="TaskbarRim-Outer"
					x={OuterInset()}
					y={OuterInset()}
					width={Math.max(0, Width() - OuterInset() * 2)}
					height={Math.max(0, Height() - OuterInset() * 2)}
					rx={OuterRadius()}
					ry={OuterRadius()}
					stroke-width={OuterWidth()}
				/>
				<rect
					class="TaskbarRim-Specular"
					x={InnerInset()}
					y={InnerInset()}
					width={Math.max(0, Width() - InnerInset() * 2)}
					height={Math.max(0, Height() - InnerInset() * 2)}
					rx={InnerRadius()}
					ry={InnerRadius()}
					stroke="url(#radius-tb-specular)"
					stroke-width={InnerWidth()}
				/>
			</svg>
		</div>
	);
};
