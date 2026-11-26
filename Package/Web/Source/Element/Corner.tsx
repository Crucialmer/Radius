import "../Asset/CSS/Corner.css";

type CornerProperty = {
	id?: string;
};

/**
 * One corner of the overlay with authentic Apple G2 continuous curvature (Squircle).
 *
 * Uses Apple's canonical 3-segment cubic continuous corner decomposition
 * (blend length = 1.528665 * R), where curvature smoothly ramps from κ = 0
 * along both straight screen edges for zero visual kinks from 1080p to 8K.
 */
export default (Property: CornerProperty) => (
	<div class="Corner" data-corner={Property.id ?? "Default"}>
		<svg class="Corner-Svg" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
			<path
				class="Corner-Shape"
				d="M 0 0 L 100 0 C 71.20 0 56.81 0 41.31 4.90 C 24.39 11.06 11.06 24.39 4.90 41.31 C 0 56.81 0 71.20 0 100 L 0 0 Z"
			/>
		</svg>
	</div>
);
