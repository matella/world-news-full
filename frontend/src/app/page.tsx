"use client";

import React, { useEffect, useState } from "react";

export default function Home() {
  const [msg, setMsg] = useState("loading...");
  useEffect(() => {
    async function fetchHello() {
      try {
        const res = await fetch("http://localhost:8000/graphql", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ query: "{ hello }" }),
        });
        const data = await res.json();
        setMsg(JSON.stringify(data));
      } catch (e) {
        setMsg("error: " + String(e));
      }
    }
    fetchHello();
  }, []);

  return (
    <main className="p-6 font-sans">
      <h1 className="text-2xl font-bold">World News — Frontend</h1>
      <p className="mt-4">GraphQL response:</p>
      <pre className="mt-2 p-3 bg-gray-100 rounded">{msg}</pre>
    </main>
  );
}
