export interface SpringOptions {
	from:number;
	to:number;
	stiffness?:number;
	damping?:number;
	precision?:number;
	clamp?:[number, number];
	onUpdate:(value:number, velocity:number) => void;
	onComplete?:() => void;
}

const ReducedMotion = ():boolean =>
	typeof window !== "undefined" &&
	typeof window.matchMedia === "function" &&
	window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * Drive a continuous damped harmonic oscillator at display refresh rate with
 * deterministic 240Hz sub-stepping so springs behave identically at 60Hz–240Hz+.
 */
export function RunSpring(options:SpringOptions):() => void {
	const {
		from,
		to,
		stiffness = 560,
		damping = 34,
		precision = 0.001,
		clamp,
		onUpdate,
		onComplete,
	} = options;

	if (ReducedMotion() || Math.abs(to - from) <= precision) {
		onUpdate(to, 0);
		onComplete?.();
		return () => {};
	}

	let value = from;
	let velocity = 0;
	let lastTime = performance.now();
	let rafId = 0;
	let cancelled = false;

	const clampVal = (v:number):number => {
		if (!clamp) return v;
		return Math.min(clamp[1], Math.max(clamp[0], v));
	};

	const tick = (now:number) => {
		if (cancelled) return;

		const rawDt = Math.min(0.05, Math.max(0.0005, (now - lastTime) / 1000));
		lastTime = now;

		const steps = Math.max(1, Math.ceil(rawDt * 240));
		const h = rawDt / steps;

		for (let i = 0; i < steps; i++) {
			const acc = stiffness * (to - value) - damping * velocity;
			velocity += acc * h;
			value = clampVal(value + velocity * h);
		}

		if (Math.abs(value - to) <= precision && Math.abs(velocity) <= precision * 10) {
			onUpdate(to, 0);
			onComplete?.();
			return;
		}

		onUpdate(value, velocity);
		rafId = requestAnimationFrame(tick);
	};

	onUpdate(from, 0);
	rafId = requestAnimationFrame(tick);

	return () => {
		cancelled = true;
		if (rafId !== 0) cancelAnimationFrame(rafId);
	};
}
