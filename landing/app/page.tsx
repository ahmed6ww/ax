'use client';

import { useEffect, useRef, useState } from 'react';
import { motion, useScroll, useTransform, useSpring } from 'framer-motion';
import Lenis from 'lenis';
import { Terminal, Cpu, Globe, Zap, ArrowDown } from 'lucide-react';

// --- COMPONENTS ---

// 1. THE VOID (HERO)
const VoidHero = () => {
  const [introComplete, setIntroComplete] = useState(false);

  return (
    <div className="h-screen relative z-10">
      <div className="h-screen overflow-hidden flex flex-col items-center justify-center bg-black text-white">

        {/* The expanding cursor - auto-animates on mount */}
        <motion.div
          initial={{ scale: 1, opacity: 1 }}
          animate={{ scale: [1, 1.5, 1, 50], opacity: [1, 1, 1, 0] }}
          transition={{ duration: 2.5, ease: "easeInOut", times: [0, 0.3, 0.6, 1] }}
          onAnimationComplete={() => setIntroComplete(true)}
          className="w-4 h-8 bg-emerald-500 origin-center z-20"
        />

        {/* Revealed Text */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 1.8, duration: 0.5 }}
          className="absolute inset-0 flex items-center justify-center z-10"
        >
          <h1 className="text-[20vw] font-black leading-none tracking-tighter text-transparent bg-clip-text bg-gradient-to-b from-white to-black mix-blend-difference">
            AX
          </h1>
        </motion.div>

        <motion.div
          initial={{ opacity: 1 }}
          animate={{ opacity: 0 }}
          transition={{ delay: 1.5, duration: 0.3 }}
          className="absolute bottom-10 left-1/2 -translate-x-1/2 text-emerald-500 font-mono text-xs tracking-widest uppercase"
        >
          Initializing...
        </motion.div>

        {/* Scroll hint after intro */}
        {introComplete && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="absolute bottom-10 left-1/2 -translate-x-1/2 text-emerald-500 font-mono text-xs tracking-widest uppercase animate-bounce"
          >
            Scroll to explore
          </motion.div>
        )}
      </div>
    </div>
  );
};


// 2. THE CHAOS (PROBLEM)
const ChaosProblem = () => {
  return (
    <section className="bg-black min-h-screen py-40 px-8 relative overflow-hidden">
      <div className="max-w-7xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-20 items-center">
          <div className="space-y-8 relative z-10">
            <h2 className="text-6xl md:text-8xl font-black text-white tracking-tighter leading-[0.8]">
              ORDER<br />
              FROM<br />
              <span className="text-red-500">CHAOS.</span>
            </h2>
            <p className="text-xl md:text-2xl text-gray-400 max-w-lg font-light leading-relaxed">
              The modern agent stack is a mess of scattered configs, disparate files, and manual plumbing.
            </p>
          </div>

          {/* Visual Chaos */}
          <div className="relative h-[600px] w-full">
            {['config.json', 'agent.yaml', 'system.md', 'context.txt', '.env.local', 'docker-compose.yml'].map((file, i) => (
              <motion.div
                key={i}
                className="absolute bg-white/5 border border-white/10 p-4 font-mono text-sm text-emerald-400 backdrop-blur-md"
                initial={{
                  x: Math.random() * 300,
                  y: Math.random() * 400,
                  rotate: Math.random() * 40 - 20
                }}
                whileInView={{
                  x: 0,
                  y: i * 80,
                  rotate: 0
                }}
                transition={{ duration: 1, delay: i * 0.1, type: "spring" }}
                viewport={{ once: true, margin: "-20%" }}
              >
                <span className="opacity-50 mr-2">📄</span> {file}
              </motion.div>
            ))}
          </div>
        </div>
      </div>
    </section>
  )
}

// 3. THE BEAM (FEATURES)
const TheBeam = () => {
  return (
    <section className="bg-black min-h-screen py-40 relative">
      <div className="absolute left-1/2 -translate-x-1/2 top-0 bottom-0 w-px bg-gradient-to-b from-transparent via-emerald-500/50 to-transparent" />

      <div className="max-w-7xl mx-auto relative z-10 space-y-40">
        {[
          { icon: <Cpu size={40} />, title: "RUST NATIVE", desc: "Blazing fast execution with zero runtime overhead." },
          { icon: <Globe size={40} />, title: "UNIVERSAL", desc: "Deploy to Claude, Cursor, or any LLM context." },
          { icon: <Zap size={40} />, title: "INSTANT", desc: "One command to rule your entire agent stack." },
        ].map((feature, i) => (
          <div key={i} className={`flex ${i % 2 === 0 ? 'justify-end' : 'justify-start'} w-full`}>
            <motion.div
              initial={{ opacity: 0, x: i % 2 === 0 ? 50 : -50 }}
              whileInView={{ opacity: 1, x: 0 }}
              transition={{ duration: 0.8 }}
              className="w-[45%] bg-white/5 border border-white/10 p-10 hover:border-emerald-500/50 transition-colors group"
            >
              <div className="text-emerald-500 mb-6 group-hover:scale-110 transition-transform origin-left">{feature.icon}</div>
              <h3 className="text-4xl font-black text-white mb-4 tracking-tighter">{feature.title}</h3>
              <p className="text-gray-400 text-lg">{feature.desc}</p>
            </motion.div>
          </div>
        ))}
      </div>
    </section>
  )
}

export default function Home() {

  useEffect(() => {
    const lenis = new Lenis()

    function raf(time: number) {
      lenis.raf(time)
      requestAnimationFrame(raf)
    }

    requestAnimationFrame(raf)
  }, [])

  return (
    <main className="bg-black text-white selection:bg-emerald-500/30 selection:text-emerald-200">
      <VoidHero />
      <ChaosProblem />
      <TheBeam />

      {/* Footer / CTA stuck to bottom for now */}
      <section className="h-[50vh] flex flex-col items-center justify-center border-t border-white/10 bg-black">
        <h2 className="text-4xl md:text-6xl font-black tracking-tighter mb-8">INITIALIZE_PROTOCOL</h2>
        <div className="flex items-center gap-4 bg-white text-black px-8 py-4 rounded-none font-bold text-xl cursor-copy hover:bg-emerald-400 transition-colors">
          <span>curl -fsSL https://ax.dev/install.sh | sh</span>
          <Terminal size={20} />
        </div>
      </section>
    </main>
  );
}
