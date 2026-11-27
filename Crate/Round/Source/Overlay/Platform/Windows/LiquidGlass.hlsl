// Radius v2.1.0 — Direct3D 11 Liquid Glass & Apple G2 Squircle Shader
// Single-pass hardware pixel shader evaluated in 1:1 physical display pixels.

cbuffer Params : register(b0) {
	float4 Screen;     // (WidthPx, HeightPx, Scale, ScreenSpanPx)
	float4 Style;      // (ScreenOpacity, InkMix, ThemeMix, BottomSpanPx)
	float4 CornerMask; // (DrawTopLeft, DrawTopRight, DrawBottomLeft, DrawBottomRight)
	float4 MaxWork;    // (MaxOpacity, WorkLeftX, WorkRightX, WorkBottomY)
	float4 RimRect;    // (RimLeft, RimTop, RimWidth, RimHeight)
	float4 RimParams;  // (RimSpanPx, RimOpacity, OuterW, InnerW)
	float4 RimDetail;  // (InnerStart, InnerEnd, RimIsHorizontal, PhaseTimeSec)
	float4 Extra;      // (0, 0, 0, 0)
};

static const float SQUIRCLE_N = 3.28;
static const float INV_SQUIRCLE_N = 1.0 / 3.28;

float4 VSMain(uint id : SV_VertexID) : SV_Position {
	float2 uv = float2((id << 1) & 2, id & 2);
	return float4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

// First-order Euclidean signed distance to the outside corner cutout of an Apple G2 squircle.
// Positive => pixel is in the clipped corner outside the rounded screen/window boundary.
float SquircleCornerMask(float2 p, float2 origin, float2 dir, float span) {
	if (span <= 0.5) {
		return 0.0;
	}

	float2 d = (p - origin) * dir;
	if (d.x < -1.5 || d.y < -1.5 || d.x > span + 1.5 || d.y > span + 1.5) {
		return 0.0;
	}

	if (d.x <= 0.0 && d.y <= 0.0) {
		return 1.0;
	}

	float2 u = saturate(1.0 - max(d, float2(0.0, 0.0)) / span);
	float2 ue = max(u, float2(1e-5, 1e-5));
	float f = pow(ue.x, SQUIRCLE_N) + pow(ue.y, SQUIRCLE_N) - 1.0;
	float2 g = float2(
		-SQUIRCLE_N * pow(ue.x, SQUIRCLE_N - 1.0) / span,
		-SQUIRCLE_N * pow(ue.y, SQUIRCLE_N - 1.0) / span
	);
	float glen = max(length(g), 1e-4);
	// Inside corner boundary => sd > 0 => cutout opaque.
	float sd = f / glen;
	return smoothstep(-0.5, 0.5, sd);
}

// Exact signed distance to an axis-aligned rectangle with Apple G2 continuous curvature corners.
// Returns float4(sd, curvatureWeight, normal.x, normal.y) where sd < 0 inside and sd == 0 on boundary.
float4 SquircleBoxSDF(float2 p, float2 center, float2 halfExt, float span) {
	float2 d = p - center;
	float2 s = float2(d.x >= 0.0 ? 1.0 : -1.0, d.y >= 0.0 ? 1.0 : -1.0);
	float2 q = abs(d) - halfExt;
	float effSpan = min(span, min(halfExt.x, halfExt.y));

	if (effSpan <= 0.5) {
		float2 w = max(q, 0.0);
		float boxDist = length(w) + min(max(q.x, q.y), 0.0);
		float2 n = (abs(q.x) > abs(q.y)) ? float2(s.x, 0.0) : float2(0.0, s.y);
		return float4(boxDist, 0.0, n);
	}

	float2 inner = halfExt - effSpan;
	float2 rel = max(abs(d) - inner, float2(0.0, 0.0));

	if (rel.x > 0.0 && rel.y > 0.0) {
		float2 u = rel / effSpan;
		float2 ue = max(u, float2(1e-5, 1e-5));
		float pn = pow(ue.x, SQUIRCLE_N) + pow(ue.y, SQUIRCLE_N);
		float f = pow(pn, INV_SQUIRCLE_N) - 1.0;

		float scale = pow(max(pn, 1e-6), INV_SQUIRCLE_N - 1.0);
		float2 df = float2(
			pow(ue.x, SQUIRCLE_N - 1.0),
			pow(ue.y, SQUIRCLE_N - 1.0)
		) * (scale / effSpan);

		float glen = max(length(df), 1e-4);
		float sd = (f / glen);
		// Recompute exact signed euclidean approximation:
		// grad(|F|) / glen keeps border stroke width uniform in physical pixels across all angles.
		float2 nrm = (df / glen) * s;
		float4 curveArg = float4(ue.x, ue.y, 0.0, 0.0);
		float curv = saturate(4.0 * u.x * u.y * 0.72 + curveArg.x * curveArg.y * 0.28);
		return float4(sd, curv, nrm);
	}

	// Straight edge region
	float edgeDist = max(q.x, q.y);
	float2 nrm = (q.x > q.y) ? float2(s.x, 0.0) : float2(0.0, s.y);
	return float4(edgeDist, 0.0, nrm);
}

// Interleaved Gradient Noise micro-dither to prevent 8-bit quantization banding on subtle glass gradients.
float IgNoise(float2 pos) {
	float3 magic = float3(0.06711056, 0.00583715, 52.9829189);
	return frac(magic.z * frac(dot(pos, magic.xy))) - 0.5;
}

float RimSpecularWeight(float depth, float startD, float endD) {
	float halfW = max((endD - startD) * 0.5, 0.35);
	float mid = (startD + endD) * 0.5;
	float distFromCenter = abs(depth - mid);
	return 1.0 - smoothstep(max(halfW - 0.55, 0.0), halfW + 0.55, distFromCenter);
}

float4 CompositeOver(float4 dstPremul, float3 srcRgb, float srcAlpha) {
	float a = saturate(srcAlpha);
	float3 srcPremul = saturate(srcRgb) * a;
	return dstPremul * (1.0 - a) + float4(srcPremul, a);
}

float4 PSMain(float4 svPos : SV_Position) : SV_Target {
	float2 p = svPos.xy;
	float W = Screen.x;
	float H = Screen.y;
	float Scale = max(Screen.z, 1.0);
	float ScreenSpan = Screen.w;

	float ScreenOp = saturate(Style.x);
	float InkMix = saturate(Style.y);
	float ThemeMix = saturate(Style.z);
	float BottomSpan = Style.w;

	float3 InkColor = lerp(float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0), InkMix);

	// 1. Screen & maximized-window Apple G2 squircle corner cutouts
	float cornerCoverage = 0.0;
	if (ScreenOp > 0.001 && ScreenSpan > 0.5) {
		if (CornerMask.x > 0.5) {
			cornerCoverage = max(cornerCoverage, SquircleCornerMask(p, float2(0.0, 0.0), float2(1.0, 1.0), ScreenSpan));
		}
		if (CornerMask.y > 0.5) {
			cornerCoverage = max(cornerCoverage, SquircleCornerMask(p, float2(W, 0.0), float2(-1.0, 1.0), ScreenSpan));
		}
		if (CornerMask.z > 0.5) {
			cornerCoverage = max(cornerCoverage, SquircleCornerMask(p, float2(0.0, H), float2(1.0, -1.0), BottomSpan));
		}
		if (CornerMask.w > 0.5) {
			cornerCoverage = max(cornerCoverage, SquircleCornerMask(p, float2(W, H), float2(-1.0, -1.0), BottomSpan));
		}
	}

	float maxCoverage = 0.0;
	float MaxOp = saturate(MaxWork.x);
	if (MaxOp > 0.001 && ScreenSpan > 0.5) {
		float WorkLeft = MaxWork.y;
		float WorkRight = MaxWork.z;
		float WorkBottom = MaxWork.w;
		maxCoverage = max(maxCoverage, SquircleCornerMask(p, float2(WorkLeft, WorkBottom), float2(1.0, -1.0), ScreenSpan));
		maxCoverage = max(maxCoverage, SquircleCornerMask(p, float2(WorkRight, WorkBottom), float2(-1.0, -1.0), ScreenSpan));
	}

	float cutoutAlpha = max(cornerCoverage * ScreenOp, maxCoverage * MaxOp);
	float4 outCol = float4(InkColor * cutoutAlpha, cutoutAlpha);

	// 2. Taskbar 4-Corner Apple G2 Squircle Liquid Glass Optical Rim
	float RimOp = saturate(RimParams.y);
	float RimSpan = RimParams.x;
	if (RimOp > 0.001 && RimSpan > 0.5 && RimRect.z > 2.0 && RimRect.w > 2.0) {
		float2 rimMin = RimRect.xy;
		float2 rimSize = RimRect.zw;
		float2 rimMax = rimMin + rimSize;

		if (p.x >= rimMin.x - 4.0 && p.x <= rimMax.x + 4.0 &&
			p.y >= rimMin.y - 4.0 && p.y <= rimMax.y + 4.0) {

			float2 center = rimMin + rimSize * 0.5;
			float2 halfExt = rimSize * 0.5;
			float4 sdf = SquircleBoxSDF(p, center, halfExt, RimSpan);

			float sd = sdf.x;
			float curv = sdf.y;
			float2 nrm = sdf.zw;
			float depth = -sd; // positive inside taskbar, 0 right on boundary

			float OuterW = RimParams.z;
			float InnerStart = RimDetail.x;
			float InnerEnd = RimDetail.y;
			bool IsHoriz = RimDetail.z > 0.5;

			// Layer A: Outer contact hairline flush along the ExtCreateRegion squircle clip boundary
			float outerCov = smoothstep(-0.55, 0.55, depth) * (1.0 - smoothstep(OuterW - 0.45, OuterW + 0.55, depth));
			float outerAlpha = outerCov * lerp(0.22, 0.38, ThemeMix) * RimOp;
			float3 outerRgb = lerp(float3(15.0 / 255.0, 18.0 / 255.0, 26.0 / 255.0), float3(0.0, 0.0, 0.0), ThemeMix);
			if (outerAlpha > 0.001) {
				outCol = CompositeOver(outCol, outerRgb, outerAlpha);
			}

			// Layer B: Specular Liquid Glass refraction rim with strict thickness-axis lighting
			// and subtle optical chromatic dispersion strongest on the 4 curved G2 corners.
			float disp = 0.35 * Scale * (0.25 + 0.75 * curv);
			float wR = RimSpecularWeight(depth - disp, InnerStart, InnerEnd);
			float wG = RimSpecularWeight(depth, InnerStart, InnerEnd);
			float wB = RimSpecularWeight(depth + disp, InnerStart, InnerEnd);
			float wAvg = (wR + wG + wB) * (1.0 / 3.0);

			if (wAvg > 0.001) {
				float topDot = IsHoriz ? saturate(-nrm.y) : saturate(-nrm.x);
				float botDot = IsHoriz ? saturate(nrm.y) : saturate(nrm.x);
				float sideDot = IsHoriz ? pow(abs(nrm.x), 1.35) : pow(abs(nrm.y), 1.35);

				float lightTop = 0.46;
				float lightSide = 0.28;
				float lightBot = 0.26;

				float darkTop = 0.28;
				float darkSide = 0.16;
				float darkBot = 0.14;

				float iLight = topDot * lightTop + sideDot * lightSide + botDot * lightBot + 0.14 * curv;
				float iDark = topDot * darkTop + sideDot * darkSide + botDot * darkBot + 0.10 * curv;
				float intensity = lerp(iLight, iDark, ThemeMix);

				// Subtle RGB glass dispersion tint (38% chroma blend around white specular)
				float3 prism = lerp(float3(wAvg, wAvg, wAvg), float3(wR, wG, wB), 0.38);
				float specAlpha = saturate(max(prism.r, max(prism.g, prism.b)) * intensity) * RimOp;
				float3 specColor = saturate(prism / max(max(prism.r, max(prism.g, prism.b)), 1e-4));

				if (specAlpha > 0.001) {
					outCol = CompositeOver(outCol, specColor, specAlpha);
				}
			}

			// Layer C: Soft inward glass depth bevel over ~3.5 * Scale physical pixels
			float innerDepth = max(depth - OuterW, 0.0);
			if (depth > OuterW - 0.5 && innerDepth < 4.0 * Scale) {
				float bevelFalloff = exp(-innerDepth / (1.8 * Scale)) * smoothstep(OuterW - 0.5, OuterW + 0.5, depth);
				float topSide = IsHoriz ? saturate(-nrm.y * 0.5 + 0.5) : saturate(-nrm.x * 0.5 + 0.5);
				float bevelBase = lerp(lerp(0.025, 0.065, topSide), lerp(0.015, 0.04, topSide), ThemeMix);
				float bevelAlpha = bevelFalloff * (bevelBase + 0.025 * curv) * RimOp;
				if (bevelAlpha > 0.001) {
					outCol = CompositeOver(outCol, float3(1.0, 1.0, 1.0), bevelAlpha);
				}
			}
		}
	}

	// Layer D: 8-bit micro-dither strictly on non-empty glass/border pixels
	if (outCol.a > 0.002) {
		float dither = IgNoise(p) * (0.45 / 255.0) * outCol.a;
		outCol.rgb = clamp(outCol.rgb + dither, 0.0, outCol.a);
	}

	outCol.a = saturate(outCol.a);
	outCol.rgb = min(outCol.rgb, outCol.a);
	return outCol;
}
