import "../Asset/CSS/Corner.css";

export type TaskbarRimProperty = {
	left: number;
	top: number;
	width: number;
	height: number;
	radius: number;
};

/**
 * Hardware-anti-aliased macOS 26 Liquid Glass rim aligned directly over the
 * clipped Windows taskbar (`Shell_TrayWnd` / `Shell_SecondaryTrayWnd`).
 *
 * Windows GDI `SetWindowRgn` clips on a 1-bit pixel grid, leaving stair-step
 * aliasing at the four rounded corners. Drawing a sub-pixel anti-aliased SVG
 * double-rim (outer seam feather + inner specular refraction highlight) directly
 * over the clipped boundary smooths the jagged pixels and gives the taskbar a
 * cohesive Liquid Glass floating-dock edge.
 */
export default (Property: TaskbarRimProperty) => {
	const Width = () => Math.max(2, Property.width);
	const Height = () => Math.max(2, Property.height);
	const Radius = () => Math.min(Property.radius, Math.min(Width(), Height()) / 2);

	return (
		<div
			class="TaskbarRim"
			style={`left: ${Property.left}px; top: ${Property.top}px; width: ${Width()}px; height: ${Height()}px;`}>
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
					x="0.35"
					y="0.35"
					width={Math.max(0, Width() - 0.7)}
					height={Math.max(0, Height() - 0.7)}
					rx={Radius()}
					ry={Radius()}
				/>
				<rect
					class="TaskbarRim-Specular"
					x="0.9"
					y="0.9"
					width={Math.max(0, Width() - 1.8)}
					height={Math.max(0, Height() - 1.8)}
					rx={Math.max(0, Radius() - 0.55)}
					ry={Math.max(0, Radius() - 0.55)}
					stroke="url(#radius-tb-specular)"
				/>
			</svg>
		</div>
	);
};
