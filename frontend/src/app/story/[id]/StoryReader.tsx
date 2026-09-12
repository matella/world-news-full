"use client";

// The readable body of a story: French by default, with a toggle to the original source-language
// text, the "where sources disagree" panel, and links out to each outlet's original article.
import { useState } from "react";
import type { Disagreement, Source } from "../../../lib/news";

export default function StoryReader({
  titleFr, bodyFr, titleOrig, bodyOrig, lang, synthesized, originCount, sourceCount,
  sources, disagreements, disagreementsFr, askUrl,
}: {
  titleFr: string;
  bodyFr: string;
  titleOrig: string;
  bodyOrig: string;
  lang: string;
  synthesized: boolean;
  originCount: number;
  sourceCount: number;
  sources: Source[];
  disagreements: Disagreement[];
  disagreementsFr: Disagreement[];
  askUrl: string;
}) {
  const hasOriginal = lang !== "fr" && (bodyOrig !== bodyFr || titleOrig !== titleFr);
  const [orig, setOrig] = useState(false);
  const showing = orig && hasOriginal;
  const title = showing ? titleOrig : titleFr;
  const body = showing ? bodyOrig : bodyFr;
  const LANG = lang.toUpperCase();

  return (
    <>
      <h1>{title}</h1>
      <div className="byline">
        <span>Synthèse · ◆ {originCount} {originCount === 1 ? "source" : "sources"}{sourceCount > originCount ? ` (${sourceCount} reprises)` : ""}</span>
        <a className="askbtn" href={askUrl} target="_blank" rel="noreferrer">
          Analyse complète par Jarvis ↗
        </a>
      </div>

      {hasOriginal && (
        <button className="toggle" onClick={() => setOrig((v) => !v)}>
          {showing ? "‹ Revenir au français" : `Voir l’original (${LANG}) ›`}
        </button>
      )}

      {synthesized && body ? (
        <div className="body">{body}</div>
      ) : body ? (
        <>
          <div className="summary">{body}</div>
          {sourceCount > 1 && (
            <p className="pending">
              ◐ {originCount} sources couvrent ce sujet — une synthèse complète est en cours de
              rédaction et apparaîtra ici sous peu.
            </p>
          )}
        </>
      ) : (
        <p className="pending">◐ Cet article est en cours de traitement — son résumé arrive.</p>
      )}

      {(() => {
        // French disagreements by default; the source-language originals when toggled to original.
        const dis = showing
          ? disagreements
          : (disagreementsFr?.length ? disagreementsFr : disagreements);
        return dis?.length > 0 && (
        <div className="panel">
          <h3>⚠ Là où les sources divergent</h3>
          {dis.map((d, i) => (
            <div className="dis" key={i}>
              <b>{d.point}</b>
              <span>{(d.positions || []).join(" · ")}</span>
            </div>
          ))}
        </div>
        );
      })()}

      {sources?.length > 0 && (
        <div className="sources">
          <h3>Lire à la source</h3>
          <ul>
            {sources.map((s, i) => (
              <li key={i}>
                <a href={s.url} target="_blank" rel="noreferrer">{s.name} ↗</a>
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}
