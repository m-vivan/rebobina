import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { CallDetectPrompt } from "./components/CallDetectPrompt";
import { RecordingBanner } from "./components/RecordingBanner";
import { UpdateBanner } from "./components/UpdateBanner";
import { ScreenPermissionSetup } from "./components/ScreenPermissionSetup";
import { Home } from "./pages/Home";
import { MeetingDetail } from "./pages/MeetingDetail";
import { Recording } from "./pages/Recording";
import { Settings } from "./pages/Settings";
import "./App.css";

function App() {
  return (
    <HashRouter>
      <div className="app-shell">
        <ScreenPermissionSetup />
        <CallDetectPrompt />
        <RecordingBanner />
        <UpdateBanner />
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/recording/:id" element={<Recording />} />
          <Route path="/meeting/:id" element={<MeetingDetail />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </div>
    </HashRouter>
  );
}

export default App;
