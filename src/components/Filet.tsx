import { Component, type ReactNode } from "react";

type Props = { children: ReactNode };
type Etat = { panne: boolean };

// Un filet de sécurité : si une page plante, on l'affiche à la place d'un écran blanc.
// Changer d'onglet ou de serveur remonte la page (la clé change) et efface la panne.
export default class Filet extends Component<Props, Etat> {
  state: Etat = { panne: false };

  static getDerivedStateFromError(): Etat {
    return { panne: true };
  }

  componentDidCatch(erreur: unknown) {
    console.error("BHOST : une page a planté", erreur);
  }

  render() {
    if (!this.state.panne) return this.props.children;
    return (
      <div className="soon">
        <b>Cette page a planté.</b>
        Change d’onglet, ou relance BHOST si ça recommence.
        <button type="button" className="btn" onClick={() => this.setState({ panne: false })}>
          Réessayer
        </button>
      </div>
    );
  }
}
