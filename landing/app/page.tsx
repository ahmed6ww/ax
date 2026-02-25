'use client';

import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import Lenis from 'lenis';
import { Terminal, Cpu, Globe, Zap, Copy, Check } from 'lucide-react';

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
  const chaosFiles = [
    { file: 'SKILL.md', initial: { x: 242, y: 304, rotate: -10 } },
    { file: '.claude/agents/*.md', initial: { x: 149, y: 140, rotate: -15 } },
    { file: '.cursor/rules/*.mdc', initial: { x: 208, y: 99, rotate: 10 } },
    { file: '~/.codex/skills/*/SKILL.md', initial: { x: 197, y: 282, rotate: 9 } },
    { file: 'mcp.json', initial: { x: 221, y: 361, rotate: -1 } },
    { file: 'config.toml', initial: { x: 56, y: 290, rotate: 19 } },
  ];

  return (
    <section className="bg-black min-h-screen py-40 px-8 relative overflow-hidden">
      <div className="max-w-7xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-20 items-center">
          <div className="space-y-8 relative z-10">
            <h2 className="text-6xl md:text-8xl font-black text-white tracking-tighter leading-[0.8]">
              TOO MUCH<br />
              AGENT<br />
              <span className="text-red-500">CHAOS.</span>
            </h2>
            <p className="text-xl md:text-2xl text-gray-400 max-w-lg font-light leading-relaxed">
              Real talk: setting up agent workflows is still copy-paste Olympics. Prompts here, MCP configs there, zero consistency.
            </p>
          </div>

          {/* Visual Chaos */}
          <div className="relative h-[600px] w-full">
            {chaosFiles.map(({ file, initial }, i) => (
              <motion.div
                key={i}
                className="absolute bg-white/5 border border-white/10 p-4 font-mono text-sm text-emerald-400 backdrop-blur-md"
                initial={initial}
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
          { icon: <Cpu size={40} />, title: "WRITE ONCE", desc: "Define a Skill once, then AX transpiles it into native formats for each editor. No weird manual rewrites." },
          { icon: <Globe size={40} />, title: "RUN ANYWHERE", desc: "Install directly to Claude Code, Cursor, or Codex from the same agent package." },
          { icon: <Zap size={40} />, title: "SHIP FASTER", desc: "ax init. ax list. ax install <agent>. Done. Less setup grind, more building." },
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

// 4. FLOW (HOW IT WORKS)
const HowItWorks = () => {
  const [activeStep, setActiveStep] = useState(0);

  const steps = [
    {
      id: "01",
      title: "INIT ONCE",
      desc: "AX scans your environment and writes a shared config so your setup is no longer random every project.",
      command: "ax init",
      signal: "~/.ax/config.toml created",
      output: [
        "→ Detecting installed editors...",
        "✓ Claude Code found",
        "✓ Cursor found",
        "✓ Config written: ~/.ax/config.toml",
      ],
    },
    {
      id: "02",
      title: "PICK AN AGENT",
      desc: "Pull a specialist from the registry instead of burning time on giant one-off system prompts.",
      command: "ax list",
      signal: "registry synced",
      output: [
        "→ Fetching registry...",
        "✓ rust-architect",
        "✓ fullstack-next",
        "✓ qa-testing-squad",
      ],
    },
    {
      id: "03",
      title: "INSTALL TO TARGET",
      desc: "AX installs native files for Claude Code, Cursor, or Codex from the same package definition.",
      command: "ax install rust-architect --target codex",
      signal: "identity + skills + MCP wired",
      output: [
        "→ Installing rust-architect...",
        "✓ Identity installed",
        "✓ 2 skill(s) installed",
        "✓ 1 MCP tool(s) configured",
      ],
    },
  ];

  const currentStep = steps[activeStep];

  return (
    <section className="bg-black py-40 px-8 border-t border-white/10">
      <div className="max-w-7xl mx-auto">
        <div className="mb-16">
          <p className="text-emerald-500 font-mono text-xs tracking-[0.25em] uppercase mb-4">Workflow</p>
          <h2 className="text-5xl md:text-7xl font-black tracking-tighter leading-[0.9] text-white">
            HOW IT
            <br />
            <span className="text-emerald-500">ACTUALLY WORKS.</span>
          </h2>
        </div>

        <div className="border border-white/10 bg-white/[0.02]">
          <div className="grid grid-cols-1 lg:grid-cols-3 border-b border-white/10">
            {steps.map((step, i) => (
              <button
                key={step.id}
                onClick={() => setActiveStep(i)}
                className={`text-left p-6 md:p-7 border-b lg:border-b-0 lg:border-r last:border-r-0 border-white/10 transition-colors ${
                  activeStep === i ? "bg-emerald-500/[0.06]" : "hover:bg-white/[0.03]"
                }`}
              >
                <p className={`font-mono text-xs tracking-[0.16em] mb-3 ${activeStep === i ? "text-emerald-400" : "text-gray-500"}`}>
                  {step.id}
                </p>
                <p className={`text-xl font-black tracking-tight ${activeStep === i ? "text-emerald-300" : "text-white"}`}>
                  {step.title}
                </p>
                <p className="font-mono text-[11px] uppercase tracking-[0.14em] text-gray-500 mt-3">
                  {step.signal}
                </p>
              </button>
            ))}
          </div>

          <motion.div
            key={currentStep.id}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.35 }}
            className="grid grid-cols-1 lg:grid-cols-[1fr_1.25fr]"
          >
            <div className="p-8 md:p-10 border-b lg:border-b-0 lg:border-r border-white/10">
              <p className="text-7xl md:text-8xl font-black tracking-tighter text-white/15 leading-none mb-6">
                {currentStep.id}
              </p>
              <h3 className="text-3xl md:text-4xl font-black tracking-tight text-white mb-5">
                {currentStep.title}
              </h3>
              <p className="text-gray-400 text-lg leading-relaxed max-w-[40ch]">
                {currentStep.desc}
              </p>
            </div>

            <div className="p-6 md:p-8">
              <div className="border border-emerald-500/30 overflow-hidden bg-black">
                <div className="flex items-center justify-between border-b border-emerald-500/20 px-4 py-3">
                  <div className="flex items-center gap-2">
                    <span className="h-2 w-2 rounded-full bg-red-400/80" />
                    <span className="h-2 w-2 rounded-full bg-yellow-400/80" />
                    <span className="h-2 w-2 rounded-full bg-emerald-400/80" />
                  </div>
                  <p className="font-mono text-[11px] tracking-[0.14em] uppercase text-emerald-400">
                    ax workflow runner
                  </p>
                </div>

                <div className="p-5 md:p-6">
                  <div className="border border-white/10 bg-black px-4 py-4 font-mono text-xs text-emerald-300 mb-4 break-all">
                    <span className="text-emerald-500 mr-2">$</span>
                    {currentStep.command}
                  </div>

                  <div className="space-y-2">
                    {currentStep.output.map((line, i) => (
                      <motion.p
                        key={`${currentStep.id}-${line}`}
                        initial={{ opacity: 0, x: -10 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ duration: 0.2, delay: i * 0.07 }}
                        className="font-mono text-xs text-gray-300"
                      >
                        {line}
                      </motion.p>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </motion.div>
        </div>
      </div>
    </section>
  );
};

// 5. USE CASES
const UseCases = () => {
  const [activeIndex, setActiveIndex] = useState(0);
  const [copied, setCopied] = useState<string | null>(null);

  const cases = [
    {
      name: "RUST-ARCHITECT",
      desc: "Tokio patterns, error handling, and systems-level guardrails for serious Rust work.",
      command: "ax install rust-architect",
      tags: ["tokio", "anyhow", "systems"],
      output: [
        "✓ Found rust-architect v1.0.0",
        "✓ Identity installed",
        "✓ 2 skill(s) installed",
        "✓ 1 MCP tool(s) configured",
      ],
    },
    {
      name: "FULLSTACK-NEXT",
      desc: "Next.js + FastAPI architecture patterns for teams shipping full-stack apps fast.",
      command: "ax install fullstack-next",
      tags: ["nextjs", "fastapi", "architecture"],
      output: [
        "✓ Found fullstack-next v1.0.0",
        "✓ Identity installed",
        "✓ 6 skill(s) installed",
        "✓ 1 MCP tool(s) configured",
      ],
    },
    {
      name: "QA-TESTING-SQUAD",
      desc: "Playwright + Jest setups when you want confidence, not flaky test chaos.",
      command: "ax install qa-testing-squad",
      tags: ["playwright", "jest", "qa"],
      output: [
        "✓ Found qa-testing-squad v1.0.0",
        "✓ Identity installed",
        "✓ 1 skill(s) installed",
        "✓ 1 MCP tool(s) configured",
      ],
    },
  ];

  const activeCase = cases[activeIndex];

  const copyCommand = async () => {
    if (typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }

    await navigator.clipboard.writeText(activeCase.command);
    setCopied(activeCase.name);
    setTimeout(() => setCopied(null), 1200);
  };

  return (
    <section className="bg-black py-40 px-8 border-t border-white/10">
      <div className="max-w-7xl mx-auto">
        <div className="mb-16">
          <p className="text-emerald-500 font-mono text-xs tracking-[0.25em] uppercase mb-4">Use Cases</p>
          <h2 className="text-5xl md:text-7xl font-black tracking-tighter leading-[0.9] text-white">
            PICK YOUR
            <br />
            <span className="text-emerald-500">EXPERT MODE.</span>
          </h2>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-[1fr_1.15fr] gap-8">
          <div className="border border-white/10 bg-white/[0.02] p-3 md:p-4">
            {cases.map((item, i) => (
              <button
                key={item.name}
                onClick={() => setActiveIndex(i)}
                className="w-full text-left p-5 md:p-6 border-b border-white/10 last:border-b-0 group cursor-pointer"
              >
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="font-mono text-xs tracking-[0.18em] text-gray-500 mb-3">
                      0{i + 1}
                    </p>
                    <h3
                      className={`text-2xl md:text-3xl font-black tracking-tight transition-colors ${
                        activeIndex === i ? "text-emerald-400" : "text-white"
                      }`}
                    >
                      {item.name}
                    </h3>
                    <p className="text-gray-400 mt-3 leading-relaxed max-w-[40ch]">{item.desc}</p>
                  </div>

                  <span
                    className={`h-2.5 w-2.5 rounded-full mt-2 transition-colors ${
                      activeIndex === i ? "bg-emerald-400" : "bg-white/20 group-hover:bg-white/40"
                    }`}
                  />
                </div>
              </button>
            ))}
          </div>

          <motion.div
            key={activeCase.name}
            initial={{ opacity: 0, x: 14 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.35 }}
            className="border border-emerald-500/30 bg-black overflow-hidden"
          >
            <div className="flex items-center justify-between border-b border-emerald-500/20 px-4 py-3">
              <div className="flex items-center gap-2">
                <span className="h-2 w-2 rounded-full bg-red-400/80" />
                <span className="h-2 w-2 rounded-full bg-yellow-400/80" />
                <span className="h-2 w-2 rounded-full bg-emerald-400/80" />
              </div>
              <p className="font-mono text-xs tracking-[0.16em] uppercase text-emerald-400">
                Active: {activeCase.name}
              </p>
            </div>

            <div className="p-6 md:p-8">
              <div className="border border-white/10 bg-black px-4 py-4 font-mono text-xs text-emerald-300 mb-5 break-all">
                <span className="text-emerald-500 mr-2">$</span>
                {activeCase.command}
              </div>

              <div className="space-y-2 mb-6">
                {activeCase.output.map((line, i) => (
                  <motion.p
                    key={line}
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.25, delay: i * 0.06 }}
                    className="font-mono text-xs text-gray-300"
                  >
                    {line}
                  </motion.p>
                ))}
              </div>

              <div className="flex flex-wrap gap-2 mb-8">
                {activeCase.tags.map((tag) => (
                  <span
                    key={tag}
                    className="px-3 py-1.5 border border-white/15 text-[11px] uppercase tracking-[0.14em] text-gray-300 font-mono"
                  >
                    {tag}
                  </span>
                ))}
              </div>

              <button
                onClick={copyCommand}
                className="inline-flex items-center gap-2 border border-white/15 px-4 py-2.5 text-xs font-mono tracking-[0.12em] uppercase text-white hover:border-emerald-500/60 hover:text-emerald-300 transition-colors"
              >
                {copied === activeCase.name ? <Check size={14} /> : <Copy size={14} />}
                {copied === activeCase.name ? "Copied" : "Copy Install Command"}
              </button>
            </div>
          </motion.div>
        </div>
      </div>
    </section>
  );
};

// 6. FAQ
const FAQ = () => {
  const faqs = [
    {
      q: "Do I need separate prompt files per editor?",
      a: "No. Define once using the Skill standard and AX writes editor-native formats during install.",
    },
    {
      q: "Which targets are supported right now?",
      a: "Claude Code, Cursor, and Codex.",
    },
    {
      q: "Can I install globally?",
      a: "Yes. Use --global to install for all projects where supported.",
    },
    {
      q: "Will AX configure MCP tools too?",
      a: "Yes. AX configures MCP entries and can prompt for API keys when a tool requires setup.",
    },
  ];

  return (
    <section className="bg-black py-40 px-8 border-t border-white/10">
      <div className="max-w-4xl mx-auto">
        <div className="mb-12">
          <p className="text-emerald-500 font-mono text-xs tracking-[0.25em] uppercase mb-4">FAQ</p>
          <h2 className="text-5xl md:text-7xl font-black tracking-tighter leading-[0.9] text-white">
            QUICK
            <br />
            <span className="text-emerald-500">ANSWERS.</span>
          </h2>
        </div>

        <div className="space-y-4">
          {faqs.map((item) => (
            <details
              key={item.q}
              className="group border border-white/10 bg-white/5 p-6 open:border-emerald-500/40"
            >
              <summary className="cursor-pointer list-none text-white font-semibold tracking-tight">
                {item.q}
              </summary>
              <p className="text-gray-400 mt-4 leading-relaxed">{item.a}</p>
            </details>
          ))}
        </div>
      </div>
    </section>
  );
};

// 7. FOOTER
const SiteFooter = () => {
  return (
    <footer className="bg-black border-t border-white/10 py-14 px-8">
      <div className="max-w-7xl mx-auto flex flex-col md:flex-row md:items-center md:justify-between gap-8">
        <div>
          <p className="text-white text-2xl font-black tracking-tight">AX</p>
          <p className="text-gray-400 mt-2">Agent Package Manager for Claude, Cursor, and Codex.</p>
        </div>

        <div className="flex items-center gap-6 text-sm font-mono text-gray-400">
          <a
            href="https://github.com/ahmed6ww/ax"
            target="_blank"
            rel="noreferrer"
            className="hover:text-emerald-400 transition-colors"
          >
            GitHub
          </a>
          <a
            href="https://github.com/ahmed6ww/ax/blob/main/LICENSE"
            target="_blank"
            rel="noreferrer"
            className="hover:text-emerald-400 transition-colors"
          >
            MIT License
          </a>
        </div>
      </div>
    </footer>
  );
};

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
        <h2 className="text-4xl md:text-6xl font-black tracking-tighter mb-8">STOP CONFIG-SPIRALING</h2>
        <div className="flex items-center gap-4 bg-white text-black px-8 py-4 rounded-none font-bold text-xl cursor-copy hover:bg-emerald-400 transition-colors">
          <span>curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | sh</span>
          <Terminal size={20} />
        </div>
      </section>
      <HowItWorks />
      <UseCases />
      <FAQ />
      <SiteFooter />
    </main>
  );
}
