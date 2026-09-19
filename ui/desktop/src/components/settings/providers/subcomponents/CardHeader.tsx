import { memo } from 'react';
import { GreenCheckButton } from './buttons/CardButtons';
import { EnabledProviderTooltipMessage, ProviderDescription } from './utils/StringUtils';
import { useIntl } from '../../../../i18n';

interface CardHeaderProps {
  name: string;
  description: string;
  isEnabled: boolean;
}

// Make CardTitle a proper React component
const CardTitle = memo(({ name }: { name: string }) => {
  return <h3 className="text-base font-medium text-text-primary truncate mr-2">{name}</h3>;
});
CardTitle.displayName = 'CardTitle';

// Properly type ProviderNameAndStatus props
interface ProviderNameAndStatusProps {
  name: string;
  isEnabled: boolean;
}

const ProviderNameAndStatus = memo(({ name, isEnabled }: ProviderNameAndStatusProps) => {
  const intl = useIntl();
  return (
    <div className="flex items-center justify-between w-full">
      <CardTitle name={name} />

      {/* Enabled state: Green check */}
      {isEnabled && <GreenCheckButton tooltip={EnabledProviderTooltipMessage(intl, name)} />}
    </div>
  );
});
ProviderNameAndStatus.displayName = 'ProviderNameAndStatus';

// Add a container div to the CardHeader
const CardHeader = memo(function CardHeader({ name, description, isEnabled }: CardHeaderProps) {
  return (
    <>
      <ProviderNameAndStatus name={name} isEnabled={isEnabled} />
      <ProviderDescription description={description} />
    </>
  );
});
CardHeader.displayName = 'CardHeader';

export default CardHeader;
