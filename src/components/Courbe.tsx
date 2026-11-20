// Une mini-courbe (64 × 24 px) pour le bandeau de statistiques.
// valeurs : les dernières mesures. max : la valeur qui touche le haut.
export default function Courbe({ valeurs, max }: { valeurs: number[]; max: number }) {
  const L = 64;
  const H = 24;
  if (valeurs.length < 2) return <svg className="spark" viewBox={`0 0 ${L} ${H}`} aria-hidden />;
  const pas = L / (valeurs.length - 1);
  const points = valeurs
    .map((v, i) => `${(i * pas).toFixed(1)},${(H - 2 - (Math.min(Math.max(v, 0), max) / max) * (H - 4)).toFixed(1)}`)
    .join(" ");
  return (
    <svg className="spark" viewBox={`0 0 ${L} ${H}`} aria-hidden>
      <polyline points={points} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
