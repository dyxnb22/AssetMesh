import { Icon } from '../../ui/Icon';
import type { AssetSummary } from './types';

/** No cover URL is stored in the current model. Keep missing artwork honest. */
export function AssetArtwork({ asset, large = false }: { asset: AssetSummary; large?: boolean }) {
  const module = asset.kind.split('.')[0];
  const game = asset.details?.module === 'media' && asset.details.media_type === 'game';
  const icon = module === 'media' ? game ? 'game' : 'media' : module === 'software' ? 'software' : module === 'service' ? 'services' : 'info';
  return (
    <div className={`asset-artwork asset-artwork-${module}${game ? ' asset-artwork-game' : ''}${large ? ' asset-artwork-large' : ''}`} aria-hidden="true">
      <Icon name={icon} size={large ? 32 : 19} />
      <span>{Array.from(asset.name.trim())[0]}</span>
    </div>
  );
}
