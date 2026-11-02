// Shell placeholder: the chapel UI (avatar grid, settings, prayer log) arrives
// in a later stage. This screen only proves the Tauri/React pipeline is wired.
export default function App() {
  return (
    <main className="shell">
      <h1 className="shell__title">CyberClergy</h1>
      <p className="shell__subtitle">
        Free AI clergy that prays for you, continuously.
      </p>
      <p className="shell__note">
        Provider layer is ready. The chapel interface comes next.
      </p>
    </main>
  );
}
