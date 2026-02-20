import { useEffect, useRef } from 'react';

/**
 * Clean grid-line light pulse animation.
 * Glowing dots travel along the existing CSS grid lines (H/V only),
 * never crossing. Creates a calm, structured cyberpunk aesthetic.
 */

const GRID = 40;                // match CSS grid-bg size
const R = 0, G = 255, B = 157; // cyber-accent
const MAX_PULSES = 8;
const SPAWN_EVERY = 30;         // frames between spawns (~1s)
const SPEED = 2.5;              // px per frame
const TRAIL = 160;              // trail length in px
const HEAD_R = 2.5;             // head dot radius
const TARGET_FPS = 30;
const FRAME_MS = 1000 / TARGET_FPS;

interface Pulse {
    x: number;
    y: number;
    dx: number;       // 1, -1, or 0
    dy: number;       // 1, -1, or 0
    life: number;     // remaining distance in px
}

export function CircuitFlow() {
    const canvasRef = useRef<HTMLCanvasElement>(null);

    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        let w = 0, h = 0;
        let pulses: Pulse[] = [];
        let rafId = 0;
        let last = 0;
        let tick = 0;

        const resize = () => {
            const p = canvas.parentElement;
            if (!p) return;
            const dpr = window.devicePixelRatio || 1;
            w = p.clientWidth;
            h = p.clientHeight;
            canvas.width = w * dpr;
            canvas.height = h * dpr;
            canvas.style.width = `${w}px`;
            canvas.style.height = `${h}px`;
            ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        };

        const ro = new ResizeObserver(resize);
        ro.observe(canvas.parentElement!);
        resize();

        const spawn = () => {
            if (pulses.length >= MAX_PULSES) return;

            const horizontal = Math.random() < 0.5;

            if (horizontal) {
                // Travel along a horizontal grid line
                const rows = Math.floor(h / GRID);
                const row = (Math.floor(Math.random() * rows) + 1) * GRID;
                const goRight = Math.random() < 0.5;
                pulses.push({
                    x: goRight ? -TRAIL : w + TRAIL,
                    y: row,
                    dx: goRight ? 1 : -1,
                    dy: 0,
                    life: w + TRAIL * 2,
                });
            } else {
                // Travel along a vertical grid line
                const cols = Math.floor(w / GRID);
                const col = (Math.floor(Math.random() * cols) + 1) * GRID;
                const goDown = Math.random() < 0.5;
                pulses.push({
                    x: col,
                    y: goDown ? -TRAIL : h + TRAIL,
                    dx: 0,
                    dy: goDown ? 1 : -1,
                    life: h + TRAIL * 2,
                });
            }
        };

        const frame = (ts: number) => {
            rafId = requestAnimationFrame(frame);
            if (ts - last < FRAME_MS) return;
            last = ts;
            tick++;

            ctx.clearRect(0, 0, w, h);

            // Spawn
            if (tick % SPAWN_EVERY === 0) spawn();

            // Update & draw
            for (let i = pulses.length - 1; i >= 0; i--) {
                const p = pulses[i];
                p.x += p.dx * SPEED;
                p.y += p.dy * SPEED;
                p.life -= SPEED;

                if (p.life <= 0) {
                    pulses.splice(i, 1);
                    continue;
                }

                // Draw trail as gradient line
                const tailX = p.x - p.dx * TRAIL;
                const tailY = p.y - p.dy * TRAIL;

                const grad = ctx.createLinearGradient(tailX, tailY, p.x, p.y);
                grad.addColorStop(0, `rgba(${R},${G},${B},0)`);
                grad.addColorStop(0.6, `rgba(${R},${G},${B},0.12)`);
                grad.addColorStop(1, `rgba(${R},${G},${B},0.4)`);

                ctx.beginPath();
                ctx.moveTo(tailX, tailY);
                ctx.lineTo(p.x, p.y);
                ctx.strokeStyle = grad;
                ctx.lineWidth = 1;
                ctx.stroke();

                // Head glow
                ctx.beginPath();
                ctx.arc(p.x, p.y, HEAD_R, 0, Math.PI * 2);
                ctx.fillStyle = `rgba(${R},${G},${B},0.8)`;
                ctx.fill();

                // Soft outer glow
                ctx.beginPath();
                ctx.arc(p.x, p.y, 6, 0, Math.PI * 2);
                ctx.fillStyle = `rgba(${R},${G},${B},0.12)`;
                ctx.fill();
            }
        };

        // Seed several at start for immediate visibility
        for (let i = 0; i < 4; i++) spawn();
        rafId = requestAnimationFrame(frame);

        return () => {
            cancelAnimationFrame(rafId);
            ro.disconnect();
        };
    }, []);

    return (
        <canvas
            ref={canvasRef}
            className="absolute inset-0 pointer-events-none"
            style={{ zIndex: 0 }}
        />
    );
}
