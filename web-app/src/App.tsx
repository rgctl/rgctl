import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Layout } from './components/Layout';
import { GraphBrowser } from './components/GraphBrowser';
import { Dashboard } from './components/Dashboard';
import { Security } from './components/Security';

function App() {
  return (
    <BrowserRouter>
      <Layout>
        <Routes>
          <Route path="/" element={<GraphBrowser />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/security" element={<Security />} />
        </Routes>
      </Layout>
    </BrowserRouter>
  );
}

export default App;
