import {
  PageFrame,
  PageTitle,
  SettingsSection,
} from "src/components/ui/page-frame";
import { SettingsNav } from "src/components/settings-nav";
import { AppearanceForm } from "./appearance-form";

export default function AppearanceSettingsPage() {
  return (
    <PageFrame>
      <PageTitle
        title="appearance"
        description="color scheme for the clotho console. follows your system when set to system."
      />
      <div className="mt-6">
        <SettingsNav active="appearance" />
      </div>

      <div className="mt-8 max-w-2xl">
        <SettingsSection
          title="color scheme"
          description="dark is the default clotho look. light inverts the belweave palette. system tracks your os preference."
        >
          <div className="px-5 py-5">
            <AppearanceForm />
          </div>
        </SettingsSection>
      </div>
    </PageFrame>
  );
}
