import "../Asset/CSS/Corner.css";

export type TaskbarRimProperty = {
	leftPct: number;
	topPct: number;
	widthPct: number;
	heightPct: number;
	widthPx: number;
	heightPx: number;
	radiusPx: number;
	scale: number;
};

/**
 * Hardware-anti-aliased macOS 26 Liquid Glass rim aligned 1:1 in physical
 * pixels directly over the clipped Windows taskbar (`Shell_TrayWnd` /
 * `Shell_SecondaryTrayWnd`).
 *
 * Positioning uses viewport percentages (`left/top/width/height` in `%`) while
 * the SVG `viewBox` uses exact physical pixels (`0 0 widthPx heightPx`) with
 * `rx = radiusPx`, making alignment completely immune to `devicePixelRatio`,
 * fractional DPI, Windows text scaling, and cross-DPI monitor hotplug drift.
 */
export default (Property: TaskbarRimProperty) => {
	const Width = () => Math.max(2, Property.widthPx);
	const Height = () => Math.max(2, Property.heightPx);
	const Scale = () => Math.max(1, Property.scale || 1);
	const Radius = () => Math.min(Property.radiusPx, Math.min(Width(), Height()) / 2);
	const OuterInset = () => 0.35 * Scale();
	const InnerInset = () => 0.9 * Scale();

	return (
		<div
			class="TaskbarRim"
			style={`left: ${Property.leftPct}%; top: ${Property.topPct}%; width: ${Property.widthPct}%; height: ${Property.heightPct}%;`}>
			<svg
				class="TaskbarRim-Svg"
				viewBox={`0 0 ${Width()} ${Height()}`}
				preserveAspectRatio="none"
				aria-hidden="true">
				<defs>
					<linearGradient id="radius-tb-specular" x1="0" y1="0" x2="0" y2="1">
						<stop offset="0%" class="TaskbarRim-StopTop" />
						<stop offset="35%" class="TaskbarRim-StopMid" />
						<stop offset="100%" class="TaskbarRim-StopBottom" />
					</linearGradient>
				</defs>
				<rect
					class="TaskbarRim-Outer"
					x={OuterInset()}
					y={OuterInset()}
					width={Math.max(0, Width() - OuterInset() * 2)}
					height={Math.max(0, Height() - OuterInset() * 2)}
					rx={Radius()}
					ry={Radius()}
					stroke-width={1.4 * Scale()}
				/>
				<rect
					class="TaskbarRim-Specular"
					x={InnerInset()}
					y={InnerInset()}
					width={Math.max(0, Width() - InnerInset() * 2)}
					height={Math.max(0, Height() - InnerInset() * 2)}
					rx={Math.max(0, Radius() - 0.55 * Scale())}
					ry={Math.max(0, Radius() - 0.55 * Scale())}
					stroke="url(#radius-tb-specular)"
					stroke-width={1.1 * Scale()}
				/>
			</svg>
		</div>
	);
};
