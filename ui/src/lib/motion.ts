// Whether charts may animate. They may not when the person asked for less motion, and not while a
// picture of a dashboard is being made (it must show the finished chart).

export const motion = { still: false };

export const reducedMotion = (): boolean => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

export const animates = (): boolean => !motion.still && !reducedMotion();
